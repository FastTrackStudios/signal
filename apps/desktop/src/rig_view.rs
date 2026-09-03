//! Signal workspace — rig picker + the chosen rig's remote.
//!
//! The picker chooses which rig to load (guitar is the only real one so
//! far; the rest of the swarm — tracks, keys, drums, bass, vocals — are
//! listed as coming). The guitar rig connects to a running signal engine
//! over a vox WebSocket (<ws://127.0.0.1:4040/vox> by default) and mounts
//! the same `GuitarRigRemote` the browser remote uses — the desktop
//! app is just another remote of the headless core. Connection lifecycle
//! is lifted from `apps/signal-web`: retry until the core answers,
//! watchdog-ping while connected, and on engine death tear down +
//! remount fresh.

use dioxus::prelude::*;
use signal_bass_proto::bass::{BassRigClient, BassRigStreamClient};
use signal_bass_ui::BassRigRemote;
use signal_drums_proto::drum::{DrumRigClient, DrumRigStreamClient};
use signal_drums_ui::DrumRigRemote;
use signal_guitar_proto::audio::AudioSettingsClient;
use signal_guitar_proto::rig::{RigClient, RigStreamClient};
use signal_guitar_ui::GuitarRigRemote;
use signal_keys_proto::keys::{KeysRigClient, KeysRigStreamClient};
use signal_keys_ui::KeysRigRemote;
use signal_synth_proto::synth::{SynthRigClient, SynthRigStreamClient};
use signal_synth_ui::SynthRigRemote;

/// Compiled Tailwind for the signal UI components (built by `just
/// tailwind` from ../input.css). This is the app's single comprehensive
/// sheet — `SessionChrome` inlines the same file for the session UI.
const SIGNAL_TAILWIND: &str = include_str!("../assets/tailwind-signal.css");

use architect::iroh_link::iroh;

use crate::remote::{EngineTarget, engine_iroh_id, establish, store_engine_iroh_id};
use crate::rigs::{Rig, RigMenu, available};

/// How this app gets a rig to talk to.
///
/// Both arms end in the same three typed clients, because the embedded engine
/// is served over an in-memory link through the very same router the network
/// one exposes. The core cannot tell which it is — the detachable-GUI rule
/// holds either way, and every view below here is unchanged.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EngineMode {
    /// In-process. No child, no port, no discovery — opening a rig plays it.
    Embedded,
    /// A separate `signal-desktop --engine`, supervised or already running.
    /// Survives this window closing and can be reached from other machines.
    Supervised,
}

impl EngineMode {
    /// Desktop defaults to embedded; the web build can only ever be a remote.
    ///
    /// Read fresh rather than cached so flipping the preference takes effect
    /// on the next rig open instead of the next launch.
    pub(crate) fn current() -> Self {
        if cfg!(target_arch = "wasm32") {
            return Self::Supervised;
        }
        match crate::prefs::get("signal-engine-mode").as_deref() {
            Some("supervised") => Self::Supervised,
            _ => Self::Embedded,
        }
    }

    pub(crate) fn store(self) {
        crate::prefs::set(
            "signal-engine-mode",
            match self {
                Self::Embedded => "embedded",
                Self::Supervised => "supervised",
            },
        );
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

/// The in-process rig's clients, bootstrapping it on first use.
///
/// Bootstrap is blocking and opens an audio device, so it runs on a blocking
/// thread rather than stalling the UI runtime. It is idempotent: the engine is
/// a process-wide `OnceLock`, so a reconnect or a remount reuses the running
/// rig instead of opening the device a second time.
#[cfg(not(target_arch = "wasm32"))]
async fn embedded_clients() -> Option<(RigClient, RigStreamClient, AudioSettingsClient)> {
    if crate::rig_engine::engine().is_none() {
        let started = tokio::task::spawn_blocking(crate::rig_engine::bootstrap_blocking)
            .await
            .map_err(|e| tracing::error!("embedded rig bootstrap panicked: {e}"))
            .ok()?;
        if let Err(e) = started {
            tracing::error!("embedded rig engine failed to start: {e:?}");
            return None;
        }
    }
    let e = crate::rig_engine::engine()?;
    Some((e.rig.clone(), e.stream.clone(), e.settings.clone()))
}

// ── Rig picker ──────────────────────────────────────────────────────────────
//
// The rig list itself lives in `crate::rigs` — one catalogue the phone shell
// renders too, so the two menus cannot drift apart again.

#[component]
pub fn SignalWorkspace() -> Element {
    let mut selected = use_signal(crate::rigs::load_last);
    // Level 1 of the chrome: which rig. The rig list is the left rail's
    // sub-rail and the crumb's menu — the `‹ Rigs` bar this used to draw was
    // a whole bar spent on one button.
    let level = fts_chrome::use_chrome_level(1);
    let chrome = level.chrome();

    let pick = use_callback(move |rig: Option<Rig>| {
        selected.set(rig);
        crate::rigs::store_last(rig);
    });

    chrome.set_sub_rail(
        Rig::ALL
            .iter()
            .copied()
            .filter(|k| available(*k))
            .map(|k| {
                fts_chrome::RailItem::new(
                    k.slug(),
                    k.label(),
                    crate::rigs::icon(k),
                    selected() == Some(k),
                    Callback::new(move |()| pick.call(Some(k))),
                )
            })
            .collect(),
    );
    // The sub-rail belongs to Signal — leaving the workspace takes it away.
    use_drop(move || chrome.set_sub_rail(Vec::new()));

    let Some(kind) = selected() else {
        level.crumbs(vec![fts_chrome::Crumb::here("Rigs")]);
        return rsx! {
            RigMenu { phone: false, on_pick: move |rig| pick.call(Some(rig)) }
        };
    };

    // The rig crumb picks any sibling rig, and "Rigs" goes back to the picker.
    level.crumbs(vec![
        fts_chrome::Crumb::new("Rigs", Callback::new(move |()| pick.call(None))),
        fts_chrome::Crumb::here(kind.label()).with_menu(
            Rig::ALL
                .iter()
                .copied()
                .filter(|k| available(*k))
                .map(|k| {
                    (
                        k.label().to_string(),
                        k == kind,
                        Callback::new(move |()| pick.call(Some(k))),
                    )
                })
                .collect(),
        ),
    ]);

    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            match kind {
                Rig::Guitar => rsx! { GuitarRigView {} },
                Rig::Bass => rsx! { BassRigView {} },
                Rig::Drums => rsx! { DrumRigView {} },
                Rig::Ekit => rsx! { crate::ekit_view::EkitView {} },
                Rig::Space => rsx! { crate::space_view::SpaceView {} },
                Rig::Keys => rsx! { KeysRigView {} },
                Rig::Synth => rsx! { SynthRigView {} },
                // Listed in the menu as not-yet; unreachable unless someone
                // hands us `--rig vocals`.
                Rig::Vocals => rsx! {
                    div { style: "flex: 1; display: flex; align-items: center; justify-content: center; \
                                  font-size: 13px; color: #71717a;",
                        "The vocal chain has no engine yet."
                    }
                },
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
            // Embedded: the rig runs here, so there is nothing to dial, retry
            // or auto-start. Bootstrap failure falls through to the remote
            // path rather than dead-ending — an engine may already be up.
            #[cfg(not(target_arch = "wasm32"))]
            if EngineMode::current() == EngineMode::Embedded {
                if let Some(c) = embedded_clients().await {
                    attempts.set(0);
                    return (generation, c);
                }
                tracing::warn!("embedded rig unavailable — falling back to a remote engine");
            }

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

/// The bass rig remote: connect over the shared `/vox` endpoint (same engine,
/// same router as guitar), provide the two clients in context, and mount
/// `BassRigRemote`. Auto-starts the local engine if nothing answers, like
/// the other rigs.
#[component]
fn BassRigView() -> Element {
    let mut generation = use_signal(|| 0u32);
    let clients = use_resource(move || {
        let generation = generation();
        async move {
            let target = EngineTarget::current();
            #[cfg(not(target_arch = "wasm32"))]
            let mut autostart = matches!(
                &target,
                EngineTarget::Ws(url) if url.contains("127.0.0.1") || url.contains("localhost")
            );
            loop {
                let rig: Option<BassRigClient> = establish(&target).await;
                let stream: Option<BassRigStreamClient> = establish(&target).await;
                if let (Some(rig), Some(stream)) = (rig, stream) {
                    return (generation, (rig, stream));
                }
                #[cfg(not(target_arch = "wasm32"))]
                if autostart {
                    autostart = false;
                    if !crate::engines::signal_running() {
                        let _ = crate::engines::start_signal();
                    }
                }
                architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
            }
        }
    });

    // Watchdog: reconnect on core loss.
    use_future(move || async move {
        let mut fails = 0u32;
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(1500)).await;
            let current = clients.peek().as_ref().cloned();
            let Some((g, (rig, _))) = current else {
                fails = 0;
                continue;
            };
            if g != *generation.peek() {
                fails = 0;
                continue;
            }
            if rig.status().await.is_ok() {
                fails = 0;
            } else {
                fails += 1;
                if fails >= 2 {
                    fails = 0;
                    generation += 1;
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
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            match state {
                Some((rig, stream)) => {
                    let _ = provide_context(rig);
                    let _ = provide_context(stream);
                    rsx! { BassRigRemote {} }
                }
                None => rsx! {
                    div { style: "display: flex; align-items: center; justify-content: center; gap: 10px; flex: 1; color: #71717a; font-size: 13px;",
                        "Connecting to the bass engine…"
                    }
                },
            }
        }
    }
}

/// The drum rig remote: connect over the shared `/vox` endpoint (same engine,
/// same router as guitar), provide the two clients in context, and mount
/// `DrumRigRemote`. On the local engine we auto-start it if nothing answers,
/// exactly like [`GuitarRigView`].
#[component]
fn DrumRigView() -> Element {
    let mut generation = use_signal(|| 0u32);
    let clients = use_resource(move || {
        let generation = generation();
        async move {
            let target = EngineTarget::current();
            #[cfg(not(target_arch = "wasm32"))]
            let mut autostart = matches!(
                &target,
                EngineTarget::Ws(url) if url.contains("127.0.0.1") || url.contains("localhost")
            );
            loop {
                let rig: Option<DrumRigClient> = establish(&target).await;
                let stream: Option<DrumRigStreamClient> = establish(&target).await;
                if let (Some(rig), Some(stream)) = (rig, stream) {
                    return (generation, (rig, stream));
                }
                #[cfg(not(target_arch = "wasm32"))]
                if autostart {
                    autostart = false;
                    if !crate::engines::signal_running() {
                        let _ = crate::engines::start_signal();
                    }
                }
                architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
            }
        }
    });

    // Watchdog: reconnect on core loss.
    use_future(move || async move {
        let mut fails = 0u32;
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(1500)).await;
            let current = clients.peek().as_ref().cloned();
            let Some((g, (rig, _))) = current else {
                fails = 0;
                continue;
            };
            if g != *generation.peek() {
                fails = 0;
                continue;
            }
            if rig.status().await.is_ok() {
                fails = 0;
            } else {
                fails += 1;
                if fails >= 2 {
                    fails = 0;
                    generation += 1;
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
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            match state {
                Some((rig, stream)) => {
                    let _ = provide_context(rig);
                    let _ = provide_context(stream);
                    rsx! { DrumRigRemote {} }
                }
                None => rsx! {
                    div { style: "display: flex; align-items: center; justify-content: center; gap: 10px; flex: 1; color: #71717a; font-size: 13px;",
                        "Connecting to the drum engine…"
                    }
                },
            }
        }
    }
}

/// The keys rig remote: connect over the shared `/vox` endpoint, provide the
/// two clients in context, and mount `KeysRigRemote`. Auto-starts the local
/// engine if nothing answers, like the other rigs.
#[component]
fn KeysRigView() -> Element {
    let mut generation = use_signal(|| 0u32);
    let clients = use_resource(move || {
        let generation = generation();
        async move {
            let target = EngineTarget::current();
            #[cfg(not(target_arch = "wasm32"))]
            let mut autostart = matches!(
                &target,
                EngineTarget::Ws(url) if url.contains("127.0.0.1") || url.contains("localhost")
            );
            loop {
                let rig: Option<KeysRigClient> = establish(&target).await;
                let stream: Option<KeysRigStreamClient> = establish(&target).await;
                if let (Some(rig), Some(stream)) = (rig, stream) {
                    return (generation, (rig, stream));
                }
                #[cfg(not(target_arch = "wasm32"))]
                if autostart {
                    autostart = false;
                    if !crate::engines::signal_running() {
                        let _ = crate::engines::start_signal();
                    }
                }
                architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
            }
        }
    });

    // Watchdog: reconnect on core loss.
    use_future(move || async move {
        let mut fails = 0u32;
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(1500)).await;
            let current = clients.peek().as_ref().cloned();
            let Some((g, (rig, _))) = current else {
                fails = 0;
                continue;
            };
            if g != *generation.peek() {
                fails = 0;
                continue;
            }
            if rig.status().await.is_ok() {
                fails = 0;
            } else {
                fails += 1;
                if fails >= 2 {
                    fails = 0;
                    generation += 1;
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
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            match state {
                Some((rig, stream)) => {
                    let _ = provide_context(rig);
                    let _ = provide_context(stream);
                    rsx! { KeysRigRemote {} }
                }
                None => rsx! {
                    div { style: "display: flex; align-items: center; justify-content: center; gap: 10px; flex: 1; color: #71717a; font-size: 13px;",
                        "Connecting to the keys engine…"
                    }
                },
            }
        }
    }
}

/// The synth rig remote: connect over the shared `/vox` endpoint, provide the
/// two clients in context, and mount `SynthRigRemote`. Auto-starts the local
/// engine if nothing answers, like the other rigs.
#[component]
fn SynthRigView() -> Element {
    let mut generation = use_signal(|| 0u32);
    let clients = use_resource(move || {
        let generation = generation();
        async move {
            let target = EngineTarget::current();
            #[cfg(not(target_arch = "wasm32"))]
            let mut autostart = matches!(
                &target,
                EngineTarget::Ws(url) if url.contains("127.0.0.1") || url.contains("localhost")
            );
            loop {
                let rig: Option<SynthRigClient> = establish(&target).await;
                let stream: Option<SynthRigStreamClient> = establish(&target).await;
                if let (Some(rig), Some(stream)) = (rig, stream) {
                    return (generation, (rig, stream));
                }
                #[cfg(not(target_arch = "wasm32"))]
                if autostart {
                    autostart = false;
                    if !crate::engines::signal_running() {
                        let _ = crate::engines::start_signal();
                    }
                }
                architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
            }
        }
    });

    // Watchdog: reconnect on core loss.
    use_future(move || async move {
        let mut fails = 0u32;
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(1500)).await;
            let current = clients.peek().as_ref().cloned();
            let Some((g, (rig, _))) = current else {
                fails = 0;
                continue;
            };
            if g != *generation.peek() {
                fails = 0;
                continue;
            }
            if rig.status().await.is_ok() {
                fails = 0;
            } else {
                fails += 1;
                if fails >= 2 {
                    fails = 0;
                    generation += 1;
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
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            match state {
                Some((rig, stream)) => {
                    let _ = provide_context(rig);
                    let _ = provide_context(stream);
                    rsx! { SynthRigRemote {} }
                }
                None => rsx! {
                    div { style: "display: flex; align-items: center; justify-content: center; gap: 10px; flex: 1; color: #71717a; font-size: 13px;",
                        "Connecting to the synth engine…"
                    }
                },
            }
        }
    }
}
