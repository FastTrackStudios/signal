//! The Session bridge: REAPER, reachable from the Session app anywhere.
//!
//! A slim REAPER extension that mounts session through [`session::host`] —
//! the same three calls `fts-extensions` makes, and nothing else of it — and
//! serves the result (REAPER's daw facade plus the setlist, mode, take and
//! record surfaces) three ways:
//!
//! - the Unix socket in `/tmp` the Mac's own tools dial;
//! - a WebSocket on the LAN, `ws://<this Mac>:4040/vox`
//!   (`FTS_SESSION_BRIDGE_PORT` to move it; a free port when that one is
//!   taken, as parallel test REAPERs find it);
//! - iroh, `fts-engine:<id>` from a key kept in
//!   `~/.config/fts/session-bridge-iroh.key`, so a phone reaches it from off
//!   the LAN too.
//!
//! An iPad, a phone or a page dials either address as it dials a Session
//! engine (the app's `connect_engine_daw`): every lane is answered by the
//! one router, so the daw facade and the setlist arrive as they would from
//! `session-desktop --engine`. The addresses are logged, written to
//! `~/.config/fts/session-bridge.txt`, and set in ExtState
//! (`FTS_SESSION_EXT`: `lan_port`, `iroh`) — the tests' way in.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use daw::module;
use daw::service::ExtState;
use daw_extension_runtime::ExtensionRuntime;
use fragile::Fragile;
use reaper_low::PluginContext;
use reaper_macros::reaper_extension_plugin;
use tracing::{info, warn};

thread_local! {
    static APP: OnceCell<Fragile<TestExtension>> = const { OnceCell::new() };
}

type ActionHandler = Arc<dyn Fn() + Send + Sync>;

struct TestExtension {
    runtime: ExtensionRuntime,
    action_rx: crossbeam_channel::Receiver<String>,
    action_handlers: HashMap<String, ActionHandler>,
}

impl TestExtension {
    fn new(context: PluginContext) -> eyre::Result<Self> {
        let runtime = ExtensionRuntime::new(context)?;

        // `runtime.build_daw()` mounts only the generic daw services
        // (`create_daw_handler()`, unmodified). `session::host::layer_router`
        // layers session's own RPC surface on top — `SetlistServiceImpl<
        // daw_reaper::Reaper>` plus the mode / take-ranking / record-control
        // surfaces — so this test extension stands in for `fts-extensions`
        // without pulling in any of its other (tempo/mirror/expression-editor/
        // ...) modules. Published on the same `/tmp/fts-daw-{pid}.sock`
        // `daw::test` already waits on, so a `#[reaper_test]`'s `ctx.daw`
        // reaches these services, and a raw `SetlistServiceClient`/
        // `SetlistServiceStreamClient` (opened the same way
        // `session-desktop`'s Recording Mode does) reaches them too.
        //
        // It has to happen inside this one `block_on`: the control surfaces
        // spawn a pump task (`tokio::spawn`) while building
        // `SessionModeServiceImpl`, which needs a tokio runtime *entered* —
        // plugin_main's bare OS thread has none, and building the router
        // outside panicked with "there is no reactor running".
        info!(
            main_thread_executor_installed = daw::main_thread::is_installed(),
            "session test extension: after ExtensionRuntime::new"
        );
        let _daw = runtime
            .handle()
            .block_on(async {
                let handler = daw_reaper::create_daw_handler();
                let handler = session::host::layer_router(handler, daw_reaper::Reaper);
                daw_reaper::socket_publisher::publish_extension_socket(handler.clone());
                // LAN test server: the exact `architect::axum_ws::serve_router`
                // path `session-desktop --engine` uses, serving this SAME
                // router — proves the real WebSocket path works against real
                // REAPER, not just the unix socket `daw::test` normally uses.
                // Port 0 (OS-assigned) so parallel test runs never collide;
                // the actual port is published via ExtState for the test to
                // discover, the same way the health beacon below announces
                // pid/status.
                spawn_bridge(&runtime, handler.clone());
                daw_reaper::build_extension_daw_with(handler).await
            })
            .map_err(|e| eyre::eyre!("{e}"))?;
        info!(
            main_thread_executor_installed = daw::main_thread::is_installed(),
            "session test extension: after build_extension_daw_with"
        );

        // `session::host` names session's whole host surface in one place, so
        // this test host and `fts-extensions` mount the same thing by
        // construction. They used to assemble it independently, and had
        // already drifted: this extension never registered session's
        // `architect::action` surface, so every test ran against a strictly
        // smaller REAPER than production without anything saying so.
        let modules = session::host::modules(daw_reaper::Reaper);
        session::host::register_actions(&daw_reaper::Reaper, daw_reaper::Reaper);
        let module_ctx = runtime.module_context();
        module::init_all(&modules, &module_ctx);
        let action_defs = module::collect_actions(&modules);

        let mut action_handlers = HashMap::new();
        for (id, _, handler, _, _) in &action_defs {
            action_handlers.insert(id.clone(), handler.clone());
        }

        let (action_tx, action_rx) = crossbeam_channel::unbounded();
        for (command_id, display_name, _, show_in_menu, toggleable) in action_defs {
            let cmd_id = daw_reaper::action_registry::register_action_main_thread(
                &command_id,
                &display_name,
                show_in_menu,
                toggleable,
            );

            if cmd_id > 0 {
                info!(command_id = %command_id, cmd_id, "session test action registered");
            } else {
                warn!(command_id = %command_id, "session test action registration returned 0");
            }
        }

        let _ = ExtState::set(
            &daw_reaper::Reaper,
            "FTS_SESSION_EXT",
            "status",
            "ready",
            false,
        );
        let _ = ExtState::set(
            &daw_reaper::Reaper,
            "FTS_SESSION_EXT",
            "pid",
            &std::process::id().to_string(),
            false,
        );

        runtime.spawn(async move {
            let mut events = daw_reaper::action_registry::subscribe_action_broadcasts();
            loop {
                match events.recv().await {
                    Ok(command_name) => {
                        let _ = action_tx.send(command_name);
                    }
                    Err(e) => {
                        warn!("session test extension action stream error: {e}");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            runtime,
            action_rx,
            action_handlers,
        })
    }

    fn timer(&self) {
        self.runtime.process_tasks();
        while let Ok(command_name) = self.action_rx.try_recv() {
            if let Some(handler) = self.action_handlers.get(&command_name) {
                handler();
            }
        }
    }
}

/// The port the LAN WebSocket listens on unless `FTS_SESSION_BRIDGE_PORT`
/// names another — `session-desktop --engine`'s, so a device dials a bridge
/// and an engine the same way.
const DEFAULT_PORT: u16 = 4040;

/// Serve `handler` on the LAN (WebSocket) and over iroh, and say where.
fn spawn_bridge(runtime: &ExtensionRuntime, handler: daw::LayerRouter) {
    let ws = handler.clone();
    runtime.spawn(async move {
        let wanted = std::env::var("FTS_SESSION_BRIDGE_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        // Taken (another REAPER, a test run beside this one): any free port.
        let listener = match tokio::net::TcpListener::bind(("0.0.0.0", wanted)).await {
            Ok(l) => l,
            Err(e) => {
                warn!(bridge.port = wanted, error = %e, "session bridge: port taken, using a free one");
                match tokio::net::TcpListener::bind(("0.0.0.0", 0)).await {
                    Ok(l) => l,
                    Err(e) => {
                        warn!(error = %e, "session bridge: the LAN server could not bind");
                        return;
                    }
                }
            }
        };
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        let _ = ExtState::set(&daw_reaper::Reaper, "FTS_SESSION_EXT", "lan_port", &port.to_string(), false);
        announce(Some(port), None);
        let app = axum::Router::new().route(
            "/vox",
            axum::routing::get(move |upgrade: axum::extract::ws::WebSocketUpgrade| {
                let handler = ws.clone();
                async move {
                    upgrade.on_upgrade(move |socket| async move {
                        architect::axum_ws::serve_router(socket, handler).await;
                    })
                }
            }),
        );
        if let Err(e) = axum::serve(listener, app).await {
            warn!(error = %e, "session bridge: the LAN server stopped");
        }
    });
    runtime.spawn(async move {
        let Some(key_path) = config_dir().map(|d| d.join("session-bridge-iroh.key")) else {
            warn!("session bridge: no config directory for the iroh key");
            return;
        };
        let key = match architect::iroh_link::load_or_create_secret_key(&key_path) {
            Ok(key) => key,
            Err(e) => {
                warn!(error = %e, "session bridge: the iroh key could not be read or made");
                return;
            }
        };
        let endpoint = match architect::iroh_link::bind_endpoint(key).await {
            Ok(endpoint) => endpoint,
            Err(e) => {
                warn!(error = %e, "session bridge: iroh did not bind");
                return;
            }
        };
        let id = endpoint.id().to_string();
        let _ = ExtState::set(&daw_reaper::Reaper, "FTS_SESSION_EXT", "iroh", &id, false);
        announce(None, Some(&id));
        architect::iroh_link::serve_router(&endpoint, handler).await;
    });
}

/// Where the bridge can be reached, said once each part is up: in the log,
/// and in `~/.config/fts/session-bridge.txt` for a person to read off.
fn announce(port: Option<u16>, iroh: Option<&str>) {
    static SEEN: std::sync::Mutex<(Option<u16>, Option<String>)> =
        std::sync::Mutex::new((None, None));
    let Ok(mut seen) = SEEN.lock() else { return };
    if port.is_some() {
        seen.0 = port;
    }
    if let Some(id) = iroh {
        seen.1 = Some(id.to_owned());
    }
    let mut lines = vec!["Session bridge — dial one of these from the Session app:".to_owned()];
    if let Some(port) = seen.0 {
        for ip in lan_addresses() {
            lines.push(format!("  ws://{ip}:{port}/vox"));
        }
        lines.push(format!("  ws://localhost:{port}/vox"));
    }
    if let Some(id) = &seen.1 {
        lines.push(format!("  fts-engine:{id}"));
    }
    let text = lines.join("\n");
    info!(
        bridge.port = seen.0,
        bridge.iroh = seen.1.as_deref(),
        "session bridge: listening"
    );
    if let Some(dir) = config_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("session-bridge.txt"), format!("{text}\n"));
    }
}

/// This Mac's LAN address, as the route to the internet leaves it (no
/// packet is sent: a UDP socket only picks the interface).
fn lan_addresses() -> Vec<String> {
    let probe = std::net::UdpSocket::bind(("0.0.0.0", 0))
        .and_then(|s| s.connect(("1.1.1.1", 80)).map(|()| s))
        .and_then(|s| s.local_addr());
    probe.map(|a| vec![a.ip().to_string()]).unwrap_or_default()
}

/// `~/.config/fts`, where the FTS tools keep their keys and notes.
fn config_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config").join("fts"))
}

extern "C" fn timer_callback() {
    APP.with(|cell| {
        if let Some(app) = cell.get() {
            app.get().timer();
        }
    });
}

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> Result<(), Box<dyn Error>> {
    init_tracing();
    info!("session test extension starting");

    let app = TestExtension::new(context)?;
    app.runtime.add_timer(timer_callback)?;

    let stored = APP.with(|cell| cell.set(Fragile::new(app)).is_ok());
    if !stored {
        return Err("session test extension already initialized".into());
    }

    info!("session test extension loaded");
    Ok(())
}

fn init_tracing() {
    let Ok(log_file) = std::fs::File::create("/tmp/session-extension.log") else {
        return;
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}
