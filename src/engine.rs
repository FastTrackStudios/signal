//! The plugin's TONE3000 engine.
//!
//! The detachable-GUI rule says every GUI is a vox remote and the core is
//! headless, and that holds here too — the plugin editor talks to a
//! `Tone3000Backend` through the generated clients, exactly as the desktop
//! app and the browser remote do. The only difference is where the backend
//! is: a plugin cannot assume a `signal-desktop --engine` is running on the
//! machine, so it serves one in-process over a memory link.
//!
//! # One engine per process, not per plugin instance
//!
//! A session with eight instances of this plugin is one user with one
//! TONE3000 account and one NAM library. The backend is a process-wide
//! `OnceLock`, so eight editors share a session, a download queue and an
//! image cache. The session file on disk is shared with the desktop app too,
//! which is the point: sign in once, anywhere.
//!
//! The runtime is leaked deliberately. A DAW may open and close an editor
//! many times in a session, and tearing a runtime down under an in-flight
//! download to rebuild it moments later buys nothing.

use std::sync::OnceLock;

use signal_tone3000::{Config, Tone3000Backend};
use signal_tone3000_proto::tone3000::{Tone3000Client, Tone3000StreamClient};

/// The established clients, plus the scope keeping the server's acceptor
/// tasks alive.
#[derive(Clone)]
pub struct Engine {
    pub client: Tone3000Client,
    pub stream: Tone3000StreamClient,
    _scope: std::sync::Arc<architect::Scope>,
}

static ENGINE: OnceLock<Option<Engine>> = OnceLock::new();

/// The process-wide engine, starting it on first use.
///
/// `None` means it could not be started; the editor then renders the
/// browser's disconnected state rather than failing to open. A plugin that
/// refuses to show its GUI because a catalog is unreachable would be a much
/// worse plugin than one that shows a message.
pub fn get() -> Option<Engine> {
    ENGINE.get_or_init(start).clone()
}

fn start() -> Option<Engine> {
    let runtime = Box::leak(Box::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("nam-tone3000")
            .enable_all()
            .build()
            .map_err(|e| tracing::warn!(%e, "nam: tone3000 runtime failed to start"))
            .ok()?,
    ));

    runtime.block_on(async {
        let config_dir = signal_sampler::rig_prefs::signal_config_dir();
        let backend = Tone3000Backend::new(Config::from_env(
            &config_dir,
            signal_nam::nam_root_from_env(&config_dir.join("nam")),
        ));
        let scope = architect::Scope::new();
        let server = architect::LocalServer::serve(backend.router(), scope.clone());

        let client: Tone3000Client = server
            .establish()
            .await
            .map_err(|e| tracing::warn!("nam: tone3000 client: {e:?}"))
            .ok()?;
        let stream: Tone3000StreamClient = server
            .establish()
            .await
            .map_err(|e| tracing::warn!("nam: tone3000 stream client: {e:?}"))
            .ok()?;

        tracing::info!("nam: in-process TONE3000 engine ready");
        Some(Engine {
            client,
            stream,
            _scope: scope,
        })
    })
}

/// Open a URL in the user's browser.
///
/// The plugin's half of the sign-in: the authorization page is a web
/// application and a Blitz editor is a renderer, so the page has to leave the
/// process entirely. Inside a DAW there is no framework service for this —
/// it is one command per platform.
pub fn open_externally(url: String) {
    if url.is_empty() {
        return;
    }
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    match std::process::Command::new(program).arg(&url).spawn() {
        Ok(_) => tracing::info!(program, "nam: opened a URL in the system browser"),
        Err(e) => tracing::warn!(program, %e, "nam: could not open the system browser"),
    }
}
