//! Shared "dial the engine" plumbing for every remote surface.
//!
//! The signal rig views (rig_view.rs) and the browser session player
//! (session_remote_view.rs) all reach `fasttrackstudio --engine` the same
//! way: one shared engine target (local WebSocket by default, a saved iroh
//! endpoint id for p2p), one typed vox client per service established over
//! its own link. This module owns that plumbing so every view connects
//! identically — moved out of rig_view.rs when the session player joined.

use architect::iroh_link::iroh;

use crate::prefs;

/// Where the engine core lives. Native: `SIGNAL_ENGINE_URL` (or legacy
/// `RIGD_URL`) at runtime, else the local default. Web: same-origin —
/// the engine that served this page also serves /vox — falling back to
/// the local default under a dev server.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn server_url() -> String {
    std::env::var("SIGNAL_ENGINE_URL")
        .or_else(|_| std::env::var("RIGD_URL"))
        .ok()
        // No env on iOS — the phone saves the engine URL from its connect UI.
        .or_else(|| prefs::get("signal-engine-ws-url"))
        .unwrap_or_else(|| "ws://127.0.0.1:4040/vox".to_string())
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn server_url() -> String {
    if let Some(saved) = prefs::get("signal-engine-ws-url") {
        return saved;
    }
    let derived = web_sys::window().and_then(|w| {
        let loc = w.location();
        let host = loc.host().ok()?; // e.g. "localhost:8080"
        let hostname = loc.hostname().ok()?; // "localhost"
        let scheme = match loc.protocol().ok()?.as_str() {
            "https:" => "wss",
            _ => "ws",
        };
        // A dev server (`dx serve`) runs the page on a non-4040 localhost port
        // and does NOT serve `/vox`; point it at the local engine so hot-reload
        // iteration connects without a manual `signal-engine-ws-url` override.
        // Deployed builds are served BY the engine (same-origin), so keep the
        // origin there.
        let is_local = hostname == "localhost" || hostname == "127.0.0.1";
        if is_local && !host.ends_with(":4040") {
            return Some("ws://127.0.0.1:4040/vox".to_string());
        }
        Some(format!("{scheme}://{host}/vox"))
    });
    derived.unwrap_or_else(|| "ws://127.0.0.1:4040/vox".to_string())
}

// ── Engine target (local ws vs remote iroh) ─────────────────────────────────

/// TEMPORARY default pack host for the phone: the studio engine's iroh
/// endpoint id (durable identity at ~/.local/share/fts-pack-host on the
/// studio machine), so a fresh install can list + download packs
/// immediately with zero setup. Remove once a proper host-pairing UX
/// exists; a ws URL or iroh id saved from the connect UI overrides it.
#[cfg(target_os = "ios")]
const DEFAULT_ENGINE_IROH_ID: &str =
    "9e16e3e074f7f3a94c1d9a95adcab1963c399e967719b7e632069cd75676dd70";

/// A saved remote engine: `SIGNAL_ENGINE_IROH_ID` at runtime (native),
/// else the id stored from the connect screen. When set, remotes dial
/// p2p over iroh instead of the WebSocket.
pub(crate) fn engine_iroh_id() -> Option<iroh::EndpointId> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(raw) = std::env::var("SIGNAL_ENGINE_IROH_ID") {
        return raw.trim().parse().ok();
    }
    if let Some(saved) = prefs::get("signal-engine-iroh-id") {
        return saved.parse().ok();
    }
    // iPhone with nothing configured: fall back to the studio pack host
    // — but never shadow an explicitly-saved ws URL.
    #[cfg(target_os = "ios")]
    if prefs::get("signal-engine-ws-url").is_none() {
        return DEFAULT_ENGINE_IROH_ID.parse().ok();
    }
    None
}

// Only the signal rig views expose a connect form today; a session-only
// build keeps the saved-id read path but never writes one.
#[cfg_attr(not(feature = "signal"), allow(dead_code))]
pub(crate) fn store_engine_iroh_id(id: Option<&str>) {
    match id {
        Some(id) => prefs::set("signal-engine-iroh-id", id.trim()),
        None => prefs::remove("signal-engine-iroh-id"),
    }
}

#[derive(Clone, PartialEq)]
pub(crate) enum EngineTarget {
    Ws(String),
    Iroh(iroh::EndpointId),
}

impl EngineTarget {
    pub(crate) fn current() -> Self {
        match engine_iroh_id() {
            Some(id) => Self::Iroh(id),
            None => Self::Ws(server_url()),
        }
    }

    pub(crate) fn label(&self) -> String {
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
    // Honor XDG_CONFIG_HOME — on iOS the app roots it under
    // Documents/FastTrackStudio (the container's ~/.config isn't
    // writable, so the HOME path would fail to create the key and no
    // iroh endpoint could ever bind).
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(xdg) => std::path::PathBuf::from(xdg),
        None => std::path::Path::new(&std::env::var_os("HOME")?).join(".config"),
    };
    let key_path = base.join("fts").join("iroh.key");
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
pub(crate) async fn establish<C: vox_core::FromVoxLane>(target: &EngineTarget) -> Option<C> {
    establish_verbose(target)
        .await
        .map_err(|e| tracing::debug!("establish {}: {e}", target.label()))
        .ok()
}

/// [`establish`] with the failure reason kept — surfaces (e.g. in the
/// phone's pack Library note) instead of vanishing into a debug log.
pub(crate) async fn establish_verbose<C: vox_core::FromVoxLane>(
    target: &EngineTarget,
) -> Result<C, String> {
    match target {
        EngineTarget::Ws(url) => {
            let link = vox_websocket::WsLink::connect(url)
                .await
                .map_err(|e| format!("ws connect {url}: {e:?}"))?;
            vox_core::initiator_on(link)
                .establish::<C>()
                .await
                .map_err(|e| format!("vox handshake: {e:?}"))
        }
        EngineTarget::Iroh(id) => {
            let ep = app_endpoint().await.ok_or("iroh endpoint bind failed")?;
            let link = architect::iroh_link::connect(&ep, *id)
                .await
                .map_err(|e| format!("iroh connect: {e}"))?;
            vox_core::initiator_on(link)
                .establish::<C>()
                .await
                .map_err(|e| format!("vox handshake (iroh): {e:?}"))
        }
    }
}
