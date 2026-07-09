//! Rig workspace — the guitar-rig remote, embedded in the app.
//!
//! Connects to a running signal engine over a vox WebSocket
//! (ws://127.0.0.1:4040/vox by default) and mounts the same
//! [`GuitarRigRemote`] the browser remote uses — the desktop app is just
//! another remote of the headless core. Connection lifecycle is lifted
//! from `apps/signal-web`: retry until the core answers, watchdog-ping
//! while connected, and on engine death tear down + remount fresh.

use dioxus::prelude::*;
use signal_guitar_proto::audio::AudioSettingsClient;
use signal_guitar_proto::rig::{RigClient, RigStreamClient};
use signal_guitar_ui::GuitarRigRemote;

/// Compiled Tailwind for the signal UI components (the same sheet the
/// web remote inlines — the app's own sheet is session-scoped).
const SIGNAL_TAILWIND: &str = include_str!("../../signal-web/assets/tailwind.css");

/// Where the rig core lives: `SIGNAL_ENGINE_URL` (or legacy `RIGD_URL`)
/// at runtime, else the local default.
fn server_url() -> String {
    std::env::var("SIGNAL_ENGINE_URL")
        .or_else(|_| std::env::var("RIGD_URL"))
        .unwrap_or_else(|_| "ws://127.0.0.1:4040/vox".to_string())
}

/// Establish one typed client over its own WebSocket (a vox caller is
/// service-bound once constructed, so sibling services don't share one).
async fn establish<C: vox_core::FromVoxLane>(url: &str) -> Option<C> {
    let link = vox_websocket::WsLink::connect(url)
        .await
        .map_err(|e| tracing::debug!("ws connect {url}: {e:?}"))
        .ok()?;
    vox_core::initiator_on(link)
        .establish::<C>()
        .await
        .map_err(|e| tracing::warn!("vox handshake: {e:?}"))
        .ok()
}

/// One connect attempt for all three clients.
async fn connect_once(url: &str) -> Option<(RigClient, RigStreamClient, AudioSettingsClient)> {
    let rig: RigClient = establish(url).await?;
    let stream: RigStreamClient = establish(url).await?;
    let settings: AudioSettingsClient = establish(url).await?;
    Some((rig, stream, settings))
}

#[component]
pub fn RigWorkspace() -> Element {
    let mut attempts = use_signal(|| 0u32);
    let mut generation = use_signal(|| 0u32);
    // The engine was up and went away (vs never seen) — changes the copy.
    let mut lost = use_signal(|| false);

    let clients = use_resource(move || {
        let generation = generation();
        async move {
            let url = server_url();
            loop {
                if let Some(c) = connect_once(&url).await {
                    attempts.set(0);
                    return (generation, c);
                }
                attempts += 1;
                architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
            }
        }
    });

    // Watchdog: ping the rig every 1.5 s. Two consecutive failures =
    // engine down → bump the generation (reconnect loop + full remount).
    use_future(move || async move {
        let mut fails = 0u32;
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(1500)).await;
            let current = clients.peek().as_ref().cloned();
            let Some((g, (rig, _, _))) = current else {
                fails = 0;
                continue;
            };
            if g != *generation.peek() {
                fails = 0;
                continue;
            }
            if rig.status().await.is_ok() {
                fails = 0;
                lost.set(false);
            } else {
                fails += 1;
                if fails >= 2 {
                    tracing::warn!("rig core lost — reconnecting");
                    fails = 0;
                    lost.set(true);
                    generation += 1; // restarts the connect resource
                }
            }
        }
    });

    let state = clients
        .read()
        .as_ref()
        .filter(|(g, _)| *g == generation())
        .map(|(_, c)| c.clone());
    rsx! {
        document::Style { {SIGNAL_TAILWIND} }
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            match state {
                Some((rig, stream, settings)) => {
                    let _ = provide_context(rig);
                    let _ = provide_context(stream);
                    let _ = provide_context(settings);
                    rsx! { GuitarRigRemote { key: "{generation}" } }
                }
                None => rsx! {
                    div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; flex: 1;",
                        span {
                            style: if lost() {
                                "width: 12px; height: 12px; border-radius: 999px; background: #ef4444;"
                            } else {
                                "width: 12px; height: 12px; border-radius: 999px; background: #22c55e;"
                            }
                        }
                        span { style: "font-size: 13px; font-weight: 600; color: #e4e4e7;",
                            if lost() { "Engine down — reconnecting…" } else { "Looking for the signal engine…" }
                        }
                        span { style: "font-size: 11px; font-family: monospace; color: #71717a;", "{server_url()}" }
                        if attempts() > 0 {
                            span { style: "font-size: 11px; color: #71717a;",
                                "Start it from the Engines area above (or `fts signal engine`) and this view will connect on its own."
                            }
                        }
                    }
                },
            }
        }
    }
}
