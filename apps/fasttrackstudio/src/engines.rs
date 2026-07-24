//! Engine supervisor — the app as engine *manager*.
//!
//! FastTrackStudio's domains run as detachable headless engines; the app
//! is a remote that can also *launch* them. This module supervises the
//! signal-engine child — this same binary re-spawned as
//! `fasttrackstudio --engine` (spawn / stop / restart, discovery via
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
    KillSignal, LaunchSource, SIGNAL_ENGINE, kill_group, probe, spawn, systemd_active,
    systemd_available, systemd_start, systemd_stop,
};

/// The signal engine child we own, if we started one.
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
/// Is this app a dev build (running out of a `target/` tree) rather than the
/// installed one?
///
/// It decides who serves the engine. A dev build must supervise its **own**
/// child — same binary, so the vox schema always matches the app, and the
/// engine dies with the window. Handing off to the systemd unit there would
/// start the *installed* binary instead: an older engine whose wire types
/// differ, which surfaces as clients that establish but then fail with
/// "writer and reader schema kinds differ" (a keys rig that connects and
/// shows nothing).
fn is_dev_build() -> bool {
    std::env::current_exe()
        .map(|p| p.components().any(|c| c.as_os_str() == "target"))
        .unwrap_or(false)
}

pub fn start_signal() -> Result<String, String> {
    if signal_running() {
        return Err("signal engine already running".into());
    }
    if systemd_available(&SIGNAL_ENGINE) && !is_dev_build() {
        systemd_start(&SIGNAL_ENGINE).map_err(|e| e.to_string())?;
        tracing::info!("signal engine started (systemd user unit)");
        return Ok(SIGNAL_ENGINE.ws_url());
    }
    // supervise: true — own process group + FTS_SUPERVISOR_PID, so closing the
    // app reaps the engine (and any grandchildren), and the engine's watchdog
    // self-exits if we die without reaping.
    let spawned = spawn(&SIGNAL_ENGINE, &[], &[], false, true)
        .map_err(|e| format!("spawn signal engine: {e}"))?;
    let how = match &spawned.source {
        LaunchSource::Binary(path) => path.display().to_string(),
        LaunchSource::Cargo => "cargo run -p fasttrackstudio -- --engine (dev fallback)".into(),
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
            // Kill the whole group (engine + any grandchildren), not just the
            // direct child; SIGTERM so the audio device closes cleanly.
            kill_group(child.id(), KillSignal::Term);
            let _ = child.wait(); // reap the direct child
            tracing::info!("signal engine stopped");
            Ok(())
        }
        None => Err("signal engine is not ours to stop (external process)".into()),
    }
}

/// Best-effort teardown when the app is shutting down: SIGTERM the owned
/// engine's process group so it doesn't outlive the window. The engine's own
/// watchdog (FTS_SUPERVISOR_PID) is the backstop if we're killed before this
/// runs. Safe to call when nothing is owned.
pub fn shutdown() {
    // systemd-managed engines are intentionally left running (a stop is final
    // and user-driven); only reap the child we spawned.
    if let Some(child) = OWNED.lock().unwrap().take() {
        tracing::info!("app shutting down — stopping owned signal engine");
        kill_group(child.id(), KillSignal::Term);
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
