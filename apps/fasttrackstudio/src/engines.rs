//! Engine supervisor — the app as engine *manager*.
//!
//! FastTrackStudio's domains run as detachable headless engines; the app
//! is a remote that can also *launch* them. This module supervises the
//! `signal-engine` child process (spawn / stop / restart, discovery via
//! the shared `engine-launcher` crate — same binary-resolution as
//! `fts signal engine`). The session engine stays in-process (see
//! `session_engine.rs`), so there is nothing to supervise for it.
//!
//! When the deployed systemd user unit exists (`just rig-install`), the
//! app starts/stops through it: crash supervision (Restart=always) while
//! running, but the app remains the on/off switch — a stop is final and
//! nothing starts at boot. Without the unit (dev tree), it falls back to
//! a direct child process. The supervisor only ever kills what it
//! controls: an engine that was already running some other way is
//! reported as "external" and left alone.

use std::process::Child;
use std::sync::Mutex;

use engine_launcher::{
    LaunchSource, SIGNAL_ENGINE, probe, spawn, systemd_active, systemd_available, systemd_start,
    systemd_stop,
};

/// The signal-engine child we own, if we started one.
static OWNED: Mutex<Option<Child>> = Mutex::new(None);

/// Is anything serving the signal engine's port (ours or external)?
pub fn signal_running() -> bool {
    probe(&SIGNAL_ENGINE)
}

/// Do we control the running signal engine (our child, or the systemd
/// unit we can stop)?
pub fn signal_owned() -> bool {
    let child_owned = {
        let mut owned = OWNED.lock().unwrap();
        match owned.as_mut() {
            // `try_wait` reaps a crashed child and clears ownership.
            Some(child) => match child.try_wait() {
                Ok(None) => true,
                _ => {
                    *owned = None;
                    false
                }
            },
            None => false,
        }
    };
    child_owned || systemd_active(&SIGNAL_ENGINE)
}

/// Start the signal engine: through its systemd unit when installed
/// (crash-supervised while running; stop stays final), else as a child
/// process attached to the app.
pub fn start_signal() -> Result<String, String> {
    if signal_running() {
        return Err("signal engine already running".into());
    }
    if systemd_available(&SIGNAL_ENGINE) {
        systemd_start(&SIGNAL_ENGINE).map_err(|e| e.to_string())?;
        tracing::info!("signal engine started (systemd user unit)");
        return Ok(SIGNAL_ENGINE.ws_url());
    }
    let spawned =
        spawn(&SIGNAL_ENGINE, &[], &[], false).map_err(|e| format!("spawn signal-engine: {e}"))?;
    let how = match &spawned.source {
        LaunchSource::Binary(path) => path.display().to_string(),
        LaunchSource::Cargo => "cargo run -p signal-engine (dev fallback)".into(),
    };
    tracing::info!("signal engine started (pid {}) via {how}", spawned.pid());
    let url = spawned.ws_url.clone();
    *OWNED.lock().unwrap() = Some(spawned.child);
    Ok(url)
}

/// Stop the signal engine — the systemd unit if it's the one running,
/// else our child process.
pub fn stop_signal() -> Result<(), String> {
    if systemd_active(&SIGNAL_ENGINE) {
        systemd_stop(&SIGNAL_ENGINE).map_err(|e| e.to_string())?;
        tracing::info!("signal engine stopped (systemd user unit)");
        return Ok(());
    }
    let mut owned = OWNED.lock().unwrap();
    match owned.take() {
        Some(mut child) => {
            child.kill().map_err(|e| format!("kill signal-engine: {e}"))?;
            let _ = child.wait(); // reap
            tracing::info!("signal engine stopped");
            Ok(())
        }
        None => Err("signal engine is not ours to stop (external process)".into()),
    }
}

/// Restart the signal engine (stop if owned, then start).
#[allow(dead_code)]
pub fn restart_signal() -> Result<String, String> {
    if signal_owned() {
        stop_signal()?;
        // Give the port a beat to free up.
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    start_signal()
}
