//! engine-launcher — locate, spawn, and probe FTS's headless engines.
//!
//! FastTrackStudio's domains run as detachable headless engines (the
//! signal engine serves the rig's vox router on `ws://:4040/vox`; a
//! standalone session gateway serves `ws://:3030/ws`). Both the unified
//! `fts` CLI (`fts signal engine`, `fts status`) and the fasttrackstudio
//! app's engine supervisor need the same three primitives, so they live
//! here once:
//!
//! - [`find_binary`]: locate an engine binary — next to the current
//!   executable first (installed layout), then `$PATH`.
//! - [`spawn`]: run it, either attached (foreground) or detached in its
//!   own session; falls back to `cargo run -p <package>` when no built
//!   binary exists (dev tree).
//! - [`probe`]: cheap TCP up/down check on an engine port.
//!
//! std-only on purpose — no tokio, no domain deps.

use std::io;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// A known headless engine: how to find it and where it listens.
#[derive(Clone, Copy, Debug)]
pub struct Engine {
    /// Human name ("Signal", "Session").
    pub name: &'static str,
    /// Binary / cargo package name (`fasttrackstudio`).
    pub package: &'static str,
    /// Arguments that select engine mode on that binary (`--engine`) —
    /// always passed before any caller args.
    pub args: &'static [&'static str],
    /// systemd user unit name (without `.service`) — kept distinct from
    /// the package so `signal-engine.service` survives the single-binary
    /// consolidation.
    pub unit: &'static str,
    /// Default TCP port it serves on.
    pub port: u16,
    /// WebSocket path on that port (`/vox`, `/ws`).
    pub ws_path: &'static str,
}

impl Engine {
    /// `ws://<host>:<port><path>` for the default local bind.
    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}{}", self.port, self.ws_path)
    }

    /// Default local socket address for probing.
    pub fn addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.port))
    }
}

/// The signal engine — the headless guitar-rig core: the fasttrackstudio
/// binary in `--engine` mode (apps/fasttrackstudio/src/engine_main.rs).
pub const SIGNAL_ENGINE: Engine = Engine {
    name: "Signal",
    package: "fasttrackstudio",
    args: &["--engine"],
    unit: "signal-engine",
    port: 4040,
    ws_path: "/vox",
};

/// The standalone session gateway (in-process engine served over ws;
/// `fts session engine`). No standalone binary exists — this entry is
/// for probing/reporting only.
pub const SESSION_ENGINE: Engine = Engine {
    name: "Session",
    package: "fts-cli", // served by `fts session engine` (in-process)
    args: &[],
    unit: "fts-cli",
    port: 3030,
    ws_path: "/ws",
};

/// Locate an engine binary: the current executable itself when it IS the
/// engine binary (the fasttrackstudio app supervising its own `--engine`
/// child — never search $PATH for ourselves), then the same directory as
/// the current executable (installed layout — everything ships side by
/// side), then $PATH.
pub fn find_binary(name: &str) -> Option<PathBuf> {
    // 1. This very executable (self-spawn: app → `<current_exe> --engine`).
    if let Ok(exe) = std::env::current_exe() {
        if exe.file_name().and_then(|n| n.to_str()) == Some(name) && exe.is_file() {
            return Some(exe);
        }
    }
    // 2. Next to the running executable.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    // 3. $PATH.
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// How a spawned engine was launched.
#[derive(Clone, Debug)]
pub enum LaunchSource {
    /// A built binary at this path.
    Binary(PathBuf),
    /// Dev fallback: `cargo run -p <package>` (no built binary found).
    Cargo,
}

/// A spawned engine child process.
pub struct Spawned {
    pub child: Child,
    pub source: LaunchSource,
    /// The ws URL the engine serves (default bind).
    pub ws_url: String,
}

impl Spawned {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

/// Spawn an engine.
///
/// Discovery order: [`find_binary`], then — if this process itself runs
/// from a cargo `target/` dir (dev tree) — `cargo run -p <package>`.
/// The current environment is propagated (plus `extra_env`).
///
/// `detach: true` puts the child in its own process group with stdio nulled,
/// so it outlives the launcher; `false` keeps it attached with inherited
/// stdio — Ctrl-C reaches it through the shared process group, the foreground
/// semantics the `fts` CLI wants.
///
/// `supervise: true` (the desktop app) makes the child OURS to reap: it runs
/// in its own process group (so [`kill_group`] takes the engine and any
/// grandchildren in one shot on our exit) and gets `FTS_SUPERVISOR_PID` so the
/// engine's own watchdog exits if we vanish — even if we're SIGKILLed and
/// never get to reap it. Independent of `detach`.
pub fn spawn(
    engine: &Engine,
    args: &[String],
    extra_env: &[(String, String)],
    detach: bool,
    supervise: bool,
) -> io::Result<Spawned> {
    let (mut cmd, source) = match find_binary(engine.package) {
        Some(bin) => {
            let mut c = Command::new(&bin);
            c.args(engine.args).args(args);
            (c, LaunchSource::Binary(bin))
        }
        None => {
            // Dev fallback: run through cargo. Only sensible when a
            // Cargo.toml is reachable from the cwd (or CARGO_MANIFEST_DIR
            // is set) — cargo itself will error clearly otherwise.
            let mut c = Command::new("cargo");
            // Audio engines always run optimized — a debug rig xruns.
            c.arg("run").arg("--release").arg("-p").arg(engine.package);
            if !engine.args.is_empty() || !args.is_empty() {
                c.arg("--").args(engine.args).args(args);
            }
            (c, LaunchSource::Cargo)
        }
    };

    for (k, v) in extra_env {
        cmd.env(k, v);
    }

    if supervise {
        // The engine's watchdog polls this pid and exits when it disappears —
        // the guarantee against orphans even if we die without reaping.
        cmd.env("FTS_SUPERVISOR_PID", std::process::id().to_string());
    }

    if detach {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }

    #[cfg(unix)]
    if detach || supervise {
        use std::os::unix::process::CommandExt;
        // Own process group (pgid == child pid). detach: survive the launcher's
        // Ctrl-C. supervise: let kill_group() take the whole subtree on exit.
        cmd.process_group(0);
    }

    let child = cmd.spawn()?;
    Ok(Spawned {
        child,
        source,
        ws_url: engine.ws_url(),
    })
}

/// Kill a supervised child's whole process group (the engine + any
/// grandchildren). `pid` is the child pid, which is also its process-group id
/// (see `spawn(supervise: true)`). No-op / best-effort on non-unix.
pub fn kill_group(pid: u32, sig: KillSignal) {
    #[cfg(unix)]
    unsafe {
        // Negative pid => the whole process group.
        libc::kill(-(pid as i32), sig as i32);
    }
    #[cfg(not(unix))]
    let _ = (pid, sig);
}

/// Signals used for engine teardown.
#[derive(Clone, Copy)]
pub enum KillSignal {
    /// Ask nicely (the engine's Drop path can flush + close the audio device).
    Term = 15,
    /// Force.
    Kill = 9,
}

/// The systemd user unit `just rig-install` installs for an engine.
fn systemd_unit(engine: &Engine) -> String {
    format!("{}.service", engine.unit)
}

/// Is the engine's systemd user unit installed on this machine?
///
/// When it is, prefer [`systemd_start`]/[`systemd_stop`] over [`spawn`]:
/// the unit keeps crash supervision (Restart=always) while the caller
/// stays the on/off switch — an explicit stop is final, systemd never
/// restarts a manually stopped unit, and nothing starts at boot unless
/// the unit is enabled (rig-install leaves it disabled).
pub fn systemd_available(engine: &Engine) -> bool {
    Command::new("systemctl")
        .args(["--user", "cat", &systemd_unit(engine)])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Is the engine's systemd user unit currently active?
pub fn systemd_active(engine: &Engine) -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", &systemd_unit(engine)])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn systemctl(verb: &str, engine: &Engine) -> io::Result<()> {
    let unit = systemd_unit(engine);
    let status = Command::new("systemctl")
        .args(["--user", verb, &unit])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "systemctl --user {verb} {unit}: {status}"
        )))
    }
}

/// Start the engine through its systemd user unit.
pub fn systemd_start(engine: &Engine) -> io::Result<()> {
    systemctl("start", engine)
}

/// Stop the engine's systemd user unit (final — no auto-restart).
pub fn systemd_stop(engine: &Engine) -> io::Result<()> {
    systemctl("stop", engine)
}

/// TCP probe: is something listening on the engine's default local port?
pub fn probe(engine: &Engine) -> bool {
    probe_addr(engine.addr(), Duration::from_millis(300))
}

/// TCP probe with explicit address + timeout.
pub fn probe_addr(addr: SocketAddr, timeout: Duration) -> bool {
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}
