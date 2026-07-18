//! `rig_engine.rs` — the signal engine embedded IN-PROCESS.
//!
//! On iOS the app cannot spawn `fasttrackstudio --engine` as a child
//! process (the `engines.rs` supervisor path), so the rig runs inside the
//! app: `GuitarRigBackend::new()` → its `LayerRouter` served over an
//! in-memory link (`architect::LocalServer`) → the SAME three typed
//! clients (`RigClient`, `RigStreamClient`, `AudioSettingsClient`) every
//! remote uses. Byte-for-byte the `session_engine.rs` pattern applied to
//! the rig — the core cannot tell it isn't a network remote.
//!
//! Desktop builds can use this too (a future "embedded engine" toggle);
//! today it is wired up by the iOS shell (`mobile_view.rs`).

use std::sync::{Arc, OnceLock};

use architect::rig::RigBackend as _;
use signal_guitar::proto::audio::AudioSettingsClient;
use signal_guitar::proto::rig::{Rig as _, RigClient, RigStreamClient};
use signal_guitar::GuitarRigBackend;

/// The embedded rig: the backend + the established in-process clients.
#[derive(Clone)]
pub struct RigEngine {
    pub rig: RigClient,
    pub stream: RigStreamClient,
    pub settings: AudioSettingsClient,
    /// Keeps the LocalServer acceptor tasks alive for the app's lifetime.
    _scope: Arc<architect::Scope>,
}

static ENGINE: OnceLock<RigEngine> = OnceLock::new();

/// The embedded rig engine, if [`bootstrap_blocking`] succeeded.
pub fn engine() -> Option<&'static RigEngine> {
    ENGINE.get()
}

/// Build the rig backend, serve it over a memory link, establish the
/// clients, and open audio. Call once before the UI launches; failure is
/// non-fatal (the rig view shows its offline notice).
///
/// Owns a dedicated leaked multi-thread runtime (the same shape as the
/// session engine): the LocalServer acceptors and vox pumps live on it for
/// the process lifetime, independent of the GUI's runtime.
pub fn bootstrap_blocking() -> eyre::Result<()> {
    if ENGINE.get().is_some() {
        return Ok(());
    }

    let runtime = Box::leak(Box::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()?,
    ));

    let engine = runtime.block_on(async {
        // The backend spawns its own OS threads (meter pump, drive
        // calibration) and opens audio off-thread on start().
        let backend = GuitarRigBackend::new();
        let router = backend.router();

        let scope = architect::Scope::new();
        let server = architect::LocalServer::serve(router, scope.clone());
        let rig: RigClient = server
            .establish()
            .await
            .map_err(|e| eyre::eyre!("rig client: {e:?}"))?;
        let stream: RigStreamClient = server
            .establish()
            .await
            .map_err(|e| eyre::eyre!("rig stream client: {e:?}"))?;
        let settings: AudioSettingsClient = server
            .establish()
            .await
            .map_err(|e| eyre::eyre!("audio settings client: {e:?}"))?;

        // Open the audio device + load the profile (returns immediately;
        // the open happens on the backend's own thread). On iOS we only
        // open when a real interface is present — otherwise there's no
        // input to route (the built-in mic is never used), and the hotplug
        // watcher opens the rig when one is connected. The perf model /
        // stacks are built in `new()`, so the UI is populated either way.
        #[cfg(not(target_os = "ios"))]
        backend.start();
        #[cfg(target_os = "ios")]
        if crate::ios_audio::has_external_input() {
            backend.start();
        }

        Ok::<_, eyre::Report>(RigEngine { rig, stream, settings, _scope: scope })
    })?;

    let _ = ENGINE.set(engine);

    // iOS: watch for audio-interface hotplug. When an external interface is
    // connected/disconnected, reconfigure the session (record vs
    // output-only — the built-in mic is never engaged) and reopen the rig
    // so cpal binds the new device. Polling is fine for device hotplug.
    #[cfg(target_os = "ios")]
    {
        let rig = ENGINE.get().unwrap().rig.clone();
        let handle = runtime.handle().clone();
        std::thread::spawn(move || {
            let mut had = crate::ios_audio::has_external_input();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                let now = crate::ios_audio::has_external_input();
                if now != had {
                    had = now;
                    tracing::info!(external = now, "audio route changed — reopening rig");
                    crate::ios_audio::configure();
                    let rig = rig.clone();
                    handle.spawn(async move {
                        let _ = rig.start().await;
                    });
                }
            }
        });
    }

    Ok(())
}
