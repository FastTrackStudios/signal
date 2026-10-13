//! Shared "dial the engine" plumbing for every remote surface.
//!
//! The browser session player (session_remote_view.rs) and the collection
//! browser (collection_browser.rs) reach `session-desktop --engine` the
//! same way: one shared engine target (local WebSocket by default, a
//! saved iroh endpoint id for p2p), one typed vox client per service
//! established over its own link. This module owns that plumbing so
//! every view connects identically.

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
        // A dev server (`dx serve`) does NOT serve `/vox`; point those
        // KNOWN dev ports at the local engine so hot-reload iteration
        // connects without a manual `signal-engine-ws-url` override.
        // Everything else — including an engine bound to a scratch port
        // via SIGNAL_ENGINE_ADDR (the e2e suite, side-by-side engines) —
        // is served BY the engine, so it stays same-origin. (The old
        // rule, "any local port that isn't 4040", silently pointed the
        // e2e's page at the LIVE :4040 engine — W7 chased phantom
        // schema-compat errors that were really an old binary.)
        let is_local = hostname == "localhost" || hostname == "127.0.0.1";
        let dx_dev_port = host.ends_with(":8080") || host.ends_with(":8087");
        if is_local && dx_dev_port {
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

// No connect form exists on this build yet; the saved-id read path stays
// live for a future one, this write path just isn't called anywhere yet.
#[allow(dead_code)]
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
    // Honor XDG_CONFIG_HOME — if the app ever roots it under
    // Documents/Session on iOS (the container's ~/.config isn't
    // writable), the HOME path below would otherwise fail to create the
    // key and no iroh endpoint could ever bind.
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

/// The app's own iroh endpoint — one per process. A persistent device
/// key is preferred (stable identity), but dialing needs none — if the
/// key file can't be created (sandboxed FS surprises) an ephemeral key
/// keeps p2p working for this launch instead of failing outright.
async fn app_endpoint() -> Result<iroh::Endpoint, String> {
    static CELL: std::sync::OnceLock<iroh::Endpoint> = std::sync::OnceLock::new();
    if let Some(ep) = CELL.get() {
        return Ok(ep.clone());
    }
    let key = device_secret_key().unwrap_or_else(|| {
        tracing::warn!("no persistent iroh key — using an ephemeral one for this launch");
        iroh::SecretKey::generate()
    });
    let ep = architect::iroh_link::bind_endpoint(key)
        .await
        .map_err(|e| format!("iroh bind: {e}"))?;
    Ok(CELL.get_or_init(|| ep).clone())
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
            let ep = app_endpoint().await?;
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

// ── A Session engine's daw facade (native) ──────────────────────────────────
//
// `session-desktop --engine` serves the whole daw facade — every service
// daw-standalone mounts, the same set REAPER's daw-bridge serves — next to
// the setlist, over its `/vox` WebSocket and over iroh. This is the client
// half: one vox connection, one lane, one `Caller` that every daw service
// client shares (the engine's router dispatches by method id, exactly like
// the REAPER extension's socket that `daw::cli::connect` dials).
//
// The result is what `daw::init_from_parts` takes, so a UI attaches to a
// Session engine the way `open::attach_to_reaper` attaches to REAPER.

/// Prefix of the connect string an engine logs for its iroh endpoint:
/// `fts-engine:<endpoint id>`.
#[cfg(not(target_arch = "wasm32"))]
pub const ENGINE_CONNECT_PREFIX: &str = "fts-engine:";

/// How long dialing an engine may take before it is reported unreachable.
/// iroh through a relay can take a few seconds on a cold endpoint.
#[cfg(not(target_arch = "wasm32"))]
const ENGINE_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Where a Session engine is reachable.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
pub enum EngineAddr {
    /// A `ws://` / `wss://` URL of the engine's `/vox` endpoint.
    Ws(String),
    /// An iroh endpoint — a bare id (resolved through the endpoint's
    /// address lookup: relays, DNS) or a full address with direct sockets.
    Iroh(iroh::EndpointAddr),
}

#[cfg(not(target_arch = "wasm32"))]
impl EngineAddr {
    /// Read what a person pastes: `ws://host:4040/vox` (a bare
    /// `ws://host:4040` gets `/vox` appended), `fts-engine:<id>`, or a bare
    /// 64-hex endpoint id.
    ///
    /// # Errors
    ///
    /// The text is none of those, or the id does not parse.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.starts_with("ws://") || text.starts_with("wss://") {
            let rest = text.split_once("://").map_or("", |(_, rest)| rest);
            return Ok(Self::Ws(if rest.contains('/') {
                text.to_string()
            } else {
                format!("{text}/vox")
            }));
        }
        let id = text.strip_prefix(ENGINE_CONNECT_PREFIX).unwrap_or(text);
        id.trim()
            .parse::<iroh::EndpointId>()
            .map(|id| Self::Iroh(id.into()))
            .map_err(|e| {
                format!("not a ws:// URL or an engine id ({ENGINE_CONNECT_PREFIX}<id>): {e}")
            })
    }

    /// The connect string for an engine's iroh endpoint.
    pub fn connect_string(id: &iroh::EndpointId) -> String {
        format!("{ENGINE_CONNECT_PREFIX}{id}")
    }

    fn label(&self) -> String {
        match self {
            Self::Ws(url) => url.clone(),
            Self::Iroh(addr) => Self::connect_string(&addr.id),
        }
    }
}

/// Captures the raw `Caller` of the one lane every daw service client
/// shares. The engine's router accepts any lane name.
#[cfg(not(target_arch = "wasm32"))]
struct EngineDawLane {
    caller: vox_core::Caller,
}

#[cfg(not(target_arch = "wasm32"))]
impl vox_core::FromVoxLane for EngineDawLane {
    const SERVICE_NAME: &'static str = "session-engine-daw";

    fn from_vox_lane(caller: vox_core::Caller, _: Option<vox_core::ConnectionHandle>) -> Self {
        Self { caller }
    }
}

/// A Session engine's daw facade, dialed. Keep it alive for as long as the
/// facade is in use — dropping it closes the connection, and every call
/// on `daw` fails from then on.
#[cfg(not(target_arch = "wasm32"))]
pub struct EngineDaw {
    /// The facade: hand a clone to `daw::init_from_parts`.
    pub daw: daw::rpc::Daw,
    /// Which engine this is, as it was dialed.
    pub addr: EngineAddr,
    _connection: vox_core::ConnectionHandle,
}

#[cfg(not(target_arch = "wasm32"))]
impl std::ops::Deref for EngineDaw {
    type Target = daw::rpc::Daw;
    fn deref(&self) -> &daw::rpc::Daw {
        &self.daw
    }
}

/// Dial a Session engine and build its daw facade. iroh addresses dial
/// from this app's own endpoint (a stable per-install identity).
///
/// The same `Caller` (`engine.daw.caller()`) also reaches the engine's
/// setlist: `session::SetlistServiceClient::new(caller.clone())`.
///
/// # Errors
///
/// The engine could not be reached, or the vox handshake failed.
#[cfg(not(target_arch = "wasm32"))]
pub async fn connect_engine_daw(addr: &EngineAddr) -> eyre::Result<EngineDaw> {
    match addr {
        EngineAddr::Ws(_) => connect_engine_daw_from(None, addr).await,
        EngineAddr::Iroh(_) => {
            let endpoint = app_endpoint().await.map_err(|e| eyre::eyre!(e))?;
            connect_engine_daw_from(Some(&endpoint), addr).await
        }
    }
}

/// [`connect_engine_daw`] from a given iroh endpoint — for a caller that
/// already owns one (a test binding loopback-only endpoints, a window
/// with its own identity). `endpoint` is only read for iroh addresses.
///
/// # Errors
///
/// As [`connect_engine_daw`]; also an iroh address with no endpoint.
#[cfg(not(target_arch = "wasm32"))]
pub async fn connect_engine_daw_from(
    endpoint: Option<&iroh::Endpoint>,
    addr: &EngineAddr,
) -> eyre::Result<EngineDaw> {
    let label = addr.label();
    let dial = async {
        let connection = match addr {
            EngineAddr::Ws(url) => {
                let link = vox_websocket::WsLink::connect(url)
                    .await
                    .map_err(|e| eyre::eyre!("ws connect {url}: {e:?}"))?;
                vox_core::initiator_on(link).establish_connection().await
            }
            EngineAddr::Iroh(remote) => {
                let endpoint = endpoint
                    .ok_or_else(|| eyre::eyre!("dialing {label} needs an iroh endpoint"))?;
                let link = architect::iroh_link::connect(endpoint, remote.clone())
                    .await
                    .map_err(|e| eyre::eyre!("iroh connect {label}: {e}"))?;
                vox_core::initiator_on(link).establish_connection().await
            }
        }
        .map_err(|e| eyre::eyre!("vox handshake with {label}: {e:?}"))?;
        let lane = connection
            .open_lane::<EngineDawLane>()
            .await
            .map_err(|e| eyre::eyre!("opening the daw lane on {label}: {e:?}"))?;
        Ok::<_, eyre::Report>(EngineDaw {
            daw: daw::rpc::Daw::new(lane.caller),
            addr: addr.clone(),
            _connection: connection,
        })
    };
    tokio::time::timeout(ENGINE_CONNECT_TIMEOUT, dial)
        .await
        .map_err(|_| eyre::eyre!("timed out dialing {label}"))?
}
