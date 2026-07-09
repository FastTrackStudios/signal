//! Engine supervisor — the app as engine *manager*.
//!
//! FastTrackStudio's domains run as detachable headless engines; the app
//! is a remote that can also *launch* them. This module supervises the
//! `signal-engine` child process (spawn / stop / restart, discovery via
//! the shared `engine-launcher` crate — same binary-resolution as
//! `fts signal engine`). The session engine stays in-process (see
//! `session_engine.rs`), so there is nothing to supervise for it.
//!
//! The supervisor only ever kills processes it spawned itself: an engine
//! that was already running (e.g. the user's live rig started with
//! `just signal-engine`) is reported as "external" and left alone.

use std::process::Child;
use std::sync::Mutex;

use engine_launcher::{LaunchSource, SIGNAL_ENGINE, probe, spawn};

/// The signal-engine child we own, if we started one.
static OWNED: Mutex<Option<Child>> = Mutex::new(None);

/// Is anything serving the signal engine's port (ours or external)?
pub fn signal_running() -> bool {
    probe(&SIGNAL_ENGINE)
}

/// Do we own the running signal-engine process?
pub fn signal_owned() -> bool {
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
}

/// Spawn the signal engine as a supervised child (attached to the app —
/// its logs share our stdout; it dies with us via the process group on
/// a terminal Ctrl-C, and [`stop_signal`] kills it explicitly).
pub fn start_signal() -> Result<String, String> {
    if signal_running() {
        return Err("signal engine already running".into());
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

/// Stop the signal engine — only if we own it.
pub fn stop_signal() -> Result<(), String> {
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
