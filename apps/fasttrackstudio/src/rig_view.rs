//! Signal workspace — rig picker + the chosen rig's remote.
//!
//! The picker chooses which rig to load (guitar is the only real one so
//! far; the rest of the swarm — tracks, keys, drums, bass, vocals — are
//! listed as coming). The guitar rig connects to a running signal engine
//! over a vox WebSocket (ws://127.0.0.1:4040/vox by default) and mounts
//! the same [`GuitarRigRemote`] the browser remote uses — the desktop
//! app is just another remote of the headless core. Connection lifecycle
//! is lifted from `apps/signal-web`: retry until the core answers,
//! watchdog-ping while connected, and on engine death tear down +
//! remount fresh.

use dioxus::prelude::*;
use signal_guitar_proto::audio::AudioSettingsClient;
use signal_guitar_proto::rig::{RigClient, RigStreamClient};
use signal_guitar_ui::GuitarRigRemote;

/// Compiled Tailwind for the signal UI components (the same sheet the
/// web remote inlines — the app's own sheet is session-scoped).
const SIGNAL_TAILWIND: &str = include_str!("../../signal-web/assets/tailwind.css");

use architect::iroh_link::iroh;

use crate::prefs;

/// Where the rig core lives. Native: `SIGNAL_ENGINE_URL` (or legacy
/// `RIGD_URL`) at runtime, else the local default. Web: same-origin —
/// the engine that served this page also serves /vox — falling back to
/// the local default under a dev server.
#[cfg(not(target_arch = "wasm32"))]
fn server_url() -> String {
    std::env::var("SIGNAL_ENGINE_URL")
        .or_else(|_| std::env::var("RIGD_URL"))
        .unwrap_or_else(|_| "ws://127.0.0.1:4040/vox".to_string())
}

#[cfg(target_arch = "wasm32")]
fn server_url() -> String {
    if let Some(saved) = prefs::get("signal-engine-ws-url") {
        return saved;
    }
    let same_origin = web_sys::window().and_then(|w| {
        let loc = w.location();
        let host = loc.host().ok()?;
        let scheme = match loc.protocol().ok()?.as_str() {
            "https:" => "wss",
            _ => "ws",
        };
        Some(format!("{scheme}://{host}/vox"))
    });
    same_origin.unwrap_or_else(|| "ws://127.0.0.1:4040/vox".to_string())
}

// ── Engine target (local ws vs remote iroh) ─────────────────────────────────

/// A saved remote engine: `SIGNAL_ENGINE_IROH_ID` at runtime (native),
/// else the id stored from the connect screen. When set, the rig dials
/// p2p over iroh instead of the WebSocket.
fn engine_iroh_id() -> Option<iroh::EndpointId> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(raw) = std::env::var("SIGNAL_ENGINE_IROH_ID") {
        return raw.trim().parse().ok();
    }
    prefs::get("signal-engine-iroh-id")?.parse().ok()
}

fn store_engine_iroh_id(id: Option<&str>) {
    match id {
        Some(id) => prefs::set("signal-engine-iroh-id", id.trim()),
        None => prefs::remove("signal-engine-iroh-id"),
    }
}

#[derive(Clone, PartialEq)]
enum EngineTarget {
    Ws(String),
    Iroh(iroh::EndpointId),
}

impl EngineTarget {
    fn current() -> Self {
        match engine_iroh_id() {
            Some(id) => Self::Iroh(id),
            None => Self::Ws(server_url()),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Ws(url) => url.clone(),
            Self::Iroh(id) => format!("iroh {id}"),
        }
    }
}

/// This device's iroh secret key — a stable identity per install.
/// Native keeps it at ~/.config/fts/iroh.key; the browser keeps it in
/// localStorage (hex).
#[cfg(not(target_arch = "wasm32"))]
fn device_secret_key() -> Option<iroh::SecretKey> {
    let home = std::env::var_os("HOME")?;
    let key_path = std::path::Path::new(&home).join(".config/fts").join("iroh.key");
    architect::iroh_link::load_or_create_secret_key(&key_path)
        .map_err(|e| tracing::error!("iroh key {key_path:?}: {e}"))
        .ok()
}

#[cfg(target_arch = "wasm32")]
fn device_secret_key() -> Option<iroh::SecretKey> {
    if let Some(hex) = prefs::get("iroh-key") {
        if let Ok(key) = architect::iroh_link::secret_key_from_hex(&hex) {
            return Some(key);
        }
    }
    let key = iroh::SecretKey::generate();
    prefs::set("iroh-key", &architect::iroh_link::secret_key_to_hex(&key));
    Some(key)
}

/// The app's own iroh endpoint — one per process.
async fn app_endpoint() -> Option<iroh::Endpoint> {
    static CELL: std::sync::OnceLock<iroh::Endpoint> = std::sync::OnceLock::new();
    if let Some(ep) = CELL.get() {
        return Some(ep.clone());
    }
    let key = device_secret_key()?;
    let ep = architect::iroh_link::bind_endpoint(key)
        .await
        .map_err(|e| tracing::error!("iroh bind: {e}"))
        .ok()?;
    Some(CELL.get_or_init(|| ep).clone())
}

/// Establish one typed client over its own link (a vox caller is
/// service-bound once constructed, so sibling services don't share one).
async fn establish<C: vox_core::FromVoxLane>(target: &EngineTarget) -> Option<C> {
    match target {
        EngineTarget::Ws(url) => {
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
        EngineTarget::Iroh(id) => {
            let ep = app_endpoint().await?;
            let link = architect::iroh_link::connect(&ep, *id)
                .await
                .map_err(|e| tracing::debug!("iroh connect {id}: {e:?}"))
                .ok()?;
            vox_core::initiator_on(link)
                .establish::<C>()
                .await
                .map_err(|e| tracing::warn!("vox handshake (iroh): {e:?}"))
                .ok()
        }
    }
}

/// One connect attempt for all three clients.
async fn connect_once(
    target: &EngineTarget,
) -> Option<(RigClient, RigStreamClient, AudioSettingsClient)> {
    let rig: RigClient = establish(target).await?;
    let stream: RigStreamClient = establish(target).await?;
    let settings: AudioSettingsClient = establish(target).await?;
    Some((rig, stream, settings))
}

// ── Rig picker ──────────────────────────────────────────────────────────────

/// The rigs the Signal workspace knows about. Guitar is the only one
/// with a real engine today; the rest are the planned swarm.
#[derive(Clone, Copy, PartialEq)]
enum RigKind {
    Guitar,
}

fn load_last_rig() -> Option<RigKind> {
    #[cfg(target_arch = "wasm32")]
    {
        let hash = web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .unwrap_or_default();
        if let Some(rig) = hash.trim_start_matches('#').split('/').nth(1) {
            if rig.eq_ignore_ascii_case("guitar") {
                return Some(RigKind::Guitar);
            }
        }
    }
    match prefs::get("last-rig").as_deref() {
        Some("guitar") => Some(RigKind::Guitar),
        _ => None,
    }
}

fn store_last_rig(rig: Option<RigKind>) {
    match rig {
        Some(RigKind::Guitar) => prefs::set("last-rig", "guitar"),
        None => prefs::remove("last-rig"),
    }
}

#[component]
pub fn SignalWorkspace() -> Element {
    let mut selected = use_signal(load_last_rig);

    match selected() {
        Some(RigKind::Guitar) => rsx! {
            div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
                div { style: "display: flex; align-items: center; gap: 8px; padding: 4px 12px; border-bottom: 1px solid #1c1c1f; font-size: 11px;",
                    button {
                        style: "padding: 2px 8px; border-radius: 5px; background: transparent; color: #a1a1aa; border: 1px solid #27272a; font-size: 11px; cursor: pointer;",
                        onclick: move |_| {
                            selected.set(None);
                            store_last_rig(None);
                        },
                        "‹ Rigs"
                    }
                    span { style: "color: #71717a;", "Guitar" }
                }
                GuitarRigView {}
            }
        },
        None => rsx! {
            RigPicker {
                on_pick: move |rig| {
                    selected.set(Some(rig));
                    store_last_rig(Some(rig));
                },
            }
        },
    }
}

#[component]
fn RigPicker(on_pick: EventHandler<RigKind>) -> Element {
    const COMING: &[(&str, &str)] = &[
        ("Tracks", "backing tracks & stems"),
        ("Keys", "keys rig"),
        ("Drums", "drum rig"),
        ("Bass", "bass rig"),
        ("Vocals", "vocal chains"),
    ];
    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 20px; flex: 1;",
            span { style: "font-size: 16px; font-weight: 700;", "Choose a rig" }
            div { style: "display: flex; gap: 12px; flex-wrap: wrap; justify-content: center; max-width: 560px;",
                button {
                    style: "display: flex; flex-direction: column; align-items: flex-start; gap: 6px; width: 170px; padding: 14px; border-radius: 10px; background: #111113; color: #e4e4e7; border: 1px solid #3f3f46; text-align: left; cursor: pointer;",
                    onclick: move |_| on_pick.call(RigKind::Guitar),
                    span { style: "font-size: 14px; font-weight: 700;", "Guitar" }
                    span { style: "font-size: 11px; color: #a1a1aa;", "amp, cab, FX — the live guitar rig" }
                    span { style: "font-size: 10px; color: #22c55e;", "default" }
                }
                for (name, desc) in COMING {
                    div { style: "display: flex; flex-direction: column; align-items: flex-start; gap: 6px; width: 170px; padding: 14px; border-radius: 10px; background: #0c0c0e; color: #52525b; border: 1px solid #1c1c1f;",
                        span { style: "font-size: 14px; font-weight: 700;", "{name}" }
                        span { style: "font-size: 11px;", "{desc}" }
                        span { style: "font-size: 10px;", "coming soon" }
                    }
                }
            }
        }
    }
}

/// Point the rig at a remote engine: paste its iroh endpoint id (the
/// engine logs it and writes ~/.config/signal/iroh-endpoint-id) and the
/// app dials it p2p from any network. Saved to
/// ~/.config/fts/signal-engine-iroh-id; clearing falls back to the
/// local WebSocket.
#[component]
fn RemoteEngineForm(generation: Signal<u32>) -> Element {
    let mut input = use_signal(String::new);
    let mut error = use_signal(String::new);
    let remote_active = engine_iroh_id().is_some();

    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; gap: 6px; margin-top: 14px; padding: 12px; border: 1px solid #1c1c1f; border-radius: 10px;",
            span { style: "font-size: 11px; color: #a1a1aa;", "Remote engine (iroh endpoint id)" }
            div { style: "display: flex; gap: 6px;",
                input {
                    style: "width: 340px; padding: 4px 8px; border-radius: 6px; background: #111113; color: #e4e4e7; border: 1px solid #27272a; font-size: 11px; font-family: monospace;",
                    placeholder: "endpoint id from the engine's startup log",
                    value: "{input}",
                    oninput: move |e| input.set(e.value()),
                }
                button {
                    style: "padding: 4px 10px; border-radius: 6px; background: transparent; color: #a1a1aa; border: 1px solid #27272a; font-size: 11px; cursor: pointer;",
                    onclick: move |_| {
                        let raw = input();
                        let raw = raw.trim();
                        if raw.parse::<iroh::EndpointId>().is_ok() {
                            store_engine_iroh_id(Some(raw));
                            error.set(String::new());
                            generation += 1;
                        } else {
                            error.set("not a valid iroh endpoint id".to_string());
                        }
                    },
                    "Connect"
                }
                if remote_active {
                    button {
                        style: "padding: 4px 10px; border-radius: 6px; background: transparent; color: #a1a1aa; border: 1px solid #27272a; font-size: 11px; cursor: pointer;",
                        onclick: move |_| {
                            store_engine_iroh_id(None);
                            error.set(String::new());
                            generation += 1;
                        },
                        "Use local"
                    }
                }
            }
            if !error().is_empty() {
                span { style: "font-size: 11px; color: #ef4444;", "{error}" }
            }
        }
    }
}

// ── Guitar rig remote ───────────────────────────────────────────────────────

#[component]
fn GuitarRigView() -> Element {
    let mut attempts = use_signal(|| 0u32);
    let mut generation = use_signal(|| 0u32);
    // The engine was up and went away (vs never seen) — changes the copy.
    let mut lost = use_signal(|| false);

    let clients = use_resource(move || {
        let generation = generation();
        async move {
            let target = EngineTarget::current();
            // Opening a rig IS the intent to run it: when the target is the
            // local engine and nothing answers, start it ourselves (native
            // only — the web build can only connect). An engine started via
            // CLI/systemd/another machine is simply connected to.
            #[cfg(not(target_arch = "wasm32"))]
            let mut autostart = matches!(
                &target,
                EngineTarget::Ws(url) if url.contains("127.0.0.1") || url.contains("localhost")
            );
            loop {
                if let Some(c) = connect_once(&target).await {
                    attempts.set(0);
                    return (generation, c);
                }
                #[cfg(not(target_arch = "wasm32"))]
                if autostart {
                    autostart = false;
                    if !crate::engines::signal_running() {
                        match crate::engines::start_signal() {
                            Ok(_) => tracing::info!("rig open: auto-started the signal engine"),
                            Err(e) => tracing::warn!("rig open: engine auto-start failed: {e}"),
                        }
                    }
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
                        span { style: "font-size: 11px; font-family: monospace; color: #71717a;", "{EngineTarget::current().label()}" }
                        if attempts() > 0 {
                            span { style: "font-size: 11px; color: #71717a;",
                                if cfg!(target_arch = "wasm32") {
                                    "Start the engine on its machine (desktop app or `fts signal engine`) and this view will connect on its own."
                                } else {
                                    "Starting the engine… (an engine started elsewhere — CLI, another machine — connects here too)"
                                }
                            }
                        }
                        RemoteEngineForm { generation }
                    }
                },
            }
        }
    }
}
