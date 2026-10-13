//! `daw` CLI — the DAW command-line surface (former `daw-cli` crate),
//! folded into the facade behind the `cli` feature.
//!
//! Provides socket discovery, connection management, track/FX resolution,
//! formatting helpers, and command implementations for querying a running
//! REAPER instance via the vox RPC protocol. The clap command tree +
//! dispatcher live in [`command`]; embedders (the `fts` CLI, the thin
//! `daw` binary) call [`cli_main`] with pre-split argv.

use std::collections::BTreeSet;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{env, fs};

use crate::rpc::Daw;
use crate::service::FxType;
use eyre::{Result, bail};
use serde_json::json;
use vox::{Caller, ConnectionHandle};

pub mod cli_values;
pub mod command;
pub mod ops;
pub mod sync;

pub use command::cli_main;

/// Minimal `FromVoxLane` client that captures the DAW service lane's
/// `Caller` (vox 0.10 replacement for the removed `NoopClient`).
#[derive(Clone)]
struct DawLaneClient {
    caller: Caller,
}

impl vox::FromVoxLane for DawLaneClient {
    const SERVICE_NAME: &'static str = "daw-cli";

    fn from_vox_lane(caller: Caller, _connection: Option<ConnectionHandle>) -> Self {
        Self { caller }
    }
}

/// A DAW connection that keeps the vox connection alive.
///
/// The `ConnectionHandle` must be kept alive for the duration of use —
/// dropping it closes the underlying vox connection and all RPC calls will fail.
pub struct DawConnection {
    pub daw: Daw,
    _connection: ConnectionHandle,
}

impl std::ops::Deref for DawConnection {
    type Target = Daw;
    fn deref(&self) -> &Daw {
        &self.daw
    }
}

// ============================================================================
// Socket Discovery
// ============================================================================

const SOCKET_DIR: &str = "/tmp";
const SOCKET_PREFIX: &str = "fts-daw-";
const SOCKET_SUFFIX: &str = ".sock";

pub fn discover_socket() -> Option<PathBuf> {
    clean_stale_daw_sockets();
    let entries = std::fs::read_dir(SOCKET_DIR).ok()?;
    let mut sockets: Vec<(u32, PathBuf)> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let filename = path.file_name()?.to_str()?;
            let rest = filename.strip_prefix(SOCKET_PREFIX)?;
            let pid_str = rest.strip_suffix(SOCKET_SUFFIX)?;
            let pid: u32 = pid_str.parse().ok()?;
            // Check if process is alive
            let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
            if alive { Some((pid, path)) } else { None }
        })
        .collect();

    // Sort by PID (most recent process likely has highest PID)
    sockets.sort_by_key(|s| std::cmp::Reverse(s.0));
    sockets.into_iter().next().map(|(_, path)| path)
}

/// Discover all DAW sockets in /tmp, returning (pid, path) pairs.
pub fn discover_all_sockets() -> Vec<(u32, PathBuf)> {
    clean_stale_daw_sockets();
    let entries = match std::fs::read_dir(SOCKET_DIR) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    let mut sockets: Vec<(u32, PathBuf)> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let filename = path.file_name()?.to_str()?;
            let rest = filename.strip_prefix(SOCKET_PREFIX)?;
            let pid_str = rest.strip_suffix(SOCKET_SUFFIX)?;
            let pid: u32 = pid_str.parse().ok()?;
            let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
            if alive { Some((pid, path)) } else { None }
        })
        .collect();
    sockets.sort_by_key(|s| std::cmp::Reverse(s.0));
    sockets
}

pub fn clean_stale_daw_sockets() {
    let Ok(entries) = fs::read_dir(SOCKET_DIR) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(rest) = filename.strip_prefix(SOCKET_PREFIX) else {
            continue;
        };
        let Some(pid_str) = rest.strip_suffix(SOCKET_SUFFIX) else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
        if !alive {
            let _ = fs::remove_file(path);
        }
    }
}

// ============================================================================
// REAPER Launcher
// ============================================================================

/// A known DAW launch profile.
#[derive(Debug, Clone)]
pub struct DawProfile {
    pub id: &'static str,
    pub label: &'static str,
    pub daw: &'static str,
    pub executable: String,
    pub resources_dir: PathBuf,
    pub ini_path: PathBuf,
    pub role: &'static str,
    pub sandboxed: bool,
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return home_dir();
    }
    path.strip_prefix("~/")
        .map(|rest| home_dir().join(rest))
        .unwrap_or_else(|| PathBuf::from(path))
}

fn which_command(name: &str) -> Option<String> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

fn default_reaper_executable(resources: &std::path::Path) -> String {
    let app_exe = resources.join("FTS-LIVE.app/Contents/MacOS/REAPER");
    if app_exe.exists() {
        return app_exe.to_string_lossy().to_string();
    }
    env::var("FTS_REAPER_EXECUTABLE")
        .ok()
        .or_else(|| which_command("reaper"))
        .or_else(macos_reaper_app)
        .unwrap_or_else(|| "reaper".to_string())
}

/// REAPER on macOS is an app bundle, not a `reaper` on PATH.
fn macos_reaper_app() -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let exe = "REAPER.app/Contents/MacOS/REAPER";
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    roots
        .into_iter()
        .map(|root| root.join(exe))
        .find(|path| path.is_file())
        .map(|path| path.to_string_lossy().to_string())
}

fn reaper_profile(
    id: &'static str,
    label: &'static str,
    resources: &str,
    role: &'static str,
    sandboxed: bool,
) -> DawProfile {
    let resources_dir = expand_home(resources);
    let ini_path = resources_dir.join("reaper.ini");

    DawProfile {
        id,
        label,
        daw: "reaper",
        executable: default_reaper_executable(&resources_dir),
        resources_dir,
        ini_path,
        role,
        sandboxed,
    }
}

/// THE three REAPER profiles. Signal rigs (guitar/keys/…) are NOT REAPER
/// profiles anymore — they live in the signal engine.
///
/// - `fts-reaper` — the main DAW instance for recording, resources at
///   `~/fasttrackstudio` (the real install; the old
///   `~/.config/FastTrackStudio/Reaper` is a compat symlink to it).
/// - `fts-tracks` — the live tracks/playback instance, own resources at
///   `~/fts-tracks` so weekday recording tweaks can't destabilize it.
/// - `fts-dev` — the isolated dev/testing copy at `~/fts-dev`.
pub fn daw_profiles() -> Vec<DawProfile> {
    vec![
        reaper_profile(
            "fts-reaper",
            "FTS REAPER (recording)",
            "~/fasttrackstudio",
            "session",
            false,
        ),
        reaper_profile(
            "fts-tracks",
            "FTS Tracks (live playback)",
            "~/fts-tracks",
            "tracks",
            false,
        ),
        reaper_profile("fts-dev", "FTS Dev REAPER", "~/fts-dev", "dev", false),
    ]
}

pub fn profile_by_id(id: &str) -> Option<DawProfile> {
    // Legacy ids from the pre-signal profile set resolve to their successors.
    let id = match id {
        "fasttrackstudio" | "fts-signal" => "fts-reaper",
        "sandbox" => "fts-dev",
        other => other,
    };
    daw_profiles().into_iter().find(|c| c.id == id)
}

fn sandbox_launcher() -> Option<String> {
    env::var("FTS_REAPER_SANDBOX")
        .ok()
        .or_else(|| env::var("FTS_REAPER_FHS").ok())
        .or_else(|| which_command("reaper-env"))
        .or_else(|| which_command("fts-test"))
}

fn ensure_reaper_profile_dirs(profile: &DawProfile) -> Result<()> {
    let user_plugins = profile.resources_dir.join("UserPlugins");
    fs::create_dir_all(&user_plugins)?;
    if let Some(parent) = profile.ini_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if !profile.ini_path.exists() {
        fs::write(&profile.ini_path, "[reaper]\n")?;
    }
    patch_reaper_profile_ini(profile)?;
    bootstrap_reaper_toolbars(profile)?;
    prewarm_reaper_profile_if_needed(profile);
    install_available_daw_bridge(&user_plugins)?;
    Ok(())
}

fn patch_reaper_profile_ini(profile: &DawProfile) -> Result<()> {
    if let Ok(audio_driver) = env::var("FTS_AUDIO_DRIVER") {
        patch_ini_key(&profile.ini_path, "audiodriver", &audio_driver)?;
    }

    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    patch_ini_section_key(&profile.ini_path, "verchk", "lastt", &now_ts.to_string())?;
    remove_stale_project_tabs(&profile.ini_path)?;
    Ok(())
}

fn bootstrap_reaper_toolbars(profile: &DawProfile) -> Result<()> {
    let menu_path = profile.resources_dir.join("reaper-menu.ini");
    let mut content = fs::read_to_string(&menu_path).unwrap_or_default();
    let mut changed = false;

    append_missing_toolbar_sections(&mut content, "Floating toolbar", 1..=32, &mut changed);
    append_missing_toolbar_sections(&mut content, "Floating MIDI toolbar", 1..=8, &mut changed);

    if changed {
        if let Some(parent) = menu_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(menu_path, content)?;
    }
    Ok(())
}

fn append_missing_toolbar_sections(
    content: &mut String,
    prefix: &str,
    range: std::ops::RangeInclusive<u8>,
    changed: &mut bool,
) {
    for toolbar in range {
        let section = format!("[{prefix} {toolbar}]");
        if content.lines().any(|line| line.trim() == section) {
            continue;
        }
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        content.push('\n');
        content.push_str(&section);
        content.push('\n');
        content.push_str("item_0=41101 Edit me\n");
        *changed = true;
    }
}

fn prewarm_reaper_profile_if_needed(profile: &DawProfile) {
    let needs_prewarm = fs::read_to_string(&profile.ini_path)
        .map(|content| !content.contains("[nag]"))
        .unwrap_or(true);
    if !needs_prewarm {
        return;
    }

    eprintln!(
        "Profile '{}' has no [nag] token yet; prewarming REAPER once so the evaluation dialog can settle.",
        profile.id
    );
    let sockets_before = discover_all_sockets()
        .into_iter()
        .map(|(_, socket)| socket)
        .collect::<BTreeSet<_>>();

    let mut cmd = if profile.sandboxed {
        let Some(launcher) = sandbox_launcher() else {
            eprintln!("Warning: cannot prewarm sandbox profile without reaper-env/fts-test");
            return;
        };
        let mut cmd = Command::new(launcher);
        cmd.arg(&profile.executable);
        cmd
    } else {
        Command::new(&profile.executable)
    };

    let spawn = cmd
        .current_dir(&profile.resources_dir)
        .env("FTS_DAW_PROFILE", profile.id)
        .env("FTS_DAW_ROLE", profile.role)
        .arg("-cfgfile")
        .arg(&profile.ini_path)
        .arg("-newinst")
        .arg("-nosplash")
        .arg("-ignoreerrors")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    let Ok(mut child) = spawn else {
        eprintln!("Warning: failed to prewarm REAPER profile '{}'", profile.id);
        return;
    };

    std::thread::sleep(std::time::Duration::from_secs(10));
    let _ = kill_reaper(child.id());
    let _ = child.wait();

    for (_, socket) in discover_all_sockets() {
        if !sockets_before.contains(&socket) {
            let _ = fs::remove_file(socket);
        }
    }

    if fs::read_to_string(&profile.ini_path)
        .map(|content| content.contains("[nag]"))
        .unwrap_or(false)
    {
        eprintln!("Profile '{}' prewarm complete.", profile.id);
    } else {
        eprintln!(
            "Warning: profile '{}' still has no [nag] token after prewarm; REAPER may show its evaluation dialog.",
            profile.id
        );
    }
}

fn patch_ini_key(path: &std::path::Path, key: &str, value: &str) -> Result<()> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let line = format!("{key}={value}");
    let patched = if content
        .lines()
        .any(|existing| existing.starts_with(&format!("{key}=")))
    {
        content
            .lines()
            .map(|existing| {
                if existing.starts_with(&format!("{key}=")) {
                    line.clone()
                } else {
                    existing.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        format!("{content}\n{line}\n")
    };
    fs::write(path, patched)?;
    Ok(())
}

fn patch_ini_section_key(
    path: &std::path::Path,
    section: &str,
    key: &str,
    value: &str,
) -> Result<()> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let section_header = format!("[{section}]");
    let key_prefix = format!("{key}=");
    let key_line = format!("{key}={value}");

    let patched = if content.lines().any(|line| line.starts_with(&key_prefix)) {
        content
            .lines()
            .map(|line| {
                if line.starts_with(&key_prefix) {
                    key_line.clone()
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else if content.contains(&section_header) {
        content.replace(&section_header, &format!("{section_header}\n{key_line}"))
    } else {
        format!("{content}\n{section_header}\n{key_line}\n")
    };

    fs::write(path, patched)?;
    Ok(())
}

fn remove_stale_project_tabs(path: &std::path::Path) -> Result<()> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let cleaned = content
        .lines()
        .filter(|line| !line.starts_with("projecttab") && !line.starts_with("lastproject="))
        .collect::<Vec<_>>()
        .join("\n");
    if cleaned != content {
        fs::write(path, cleaned)?;
    }
    Ok(())
}

fn install_available_daw_bridge(user_plugins: &std::path::Path) -> Result<()> {
    let Some(source) = find_built_daw_bridge() else {
        eprintln!(
            "Warning: daw-bridge is not built yet; run `cargo build -p daw-bridge` before launching if this profile needs CLI control."
        );
        return Ok(());
    };

    let dest = user_plugins.join(format!("reaper_daw_bridge.{}", env::consts::DLL_EXTENSION));
    if fs::read_link(&dest).ok().as_deref() == Some(source.as_path()) {
        return Ok(());
    }
    let _ = fs::remove_file(&dest);

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&source, &dest).or_else(|_| {
            fs::copy(&source, &dest)?;
            Ok::<(), std::io::Error>(())
        })?;
    }
    #[cfg(not(unix))]
    {
        fs::copy(&source, &dest)?;
    }
    Ok(())
}

fn find_built_daw_bridge() -> Option<PathBuf> {
    let lib = format!(
        "{}reaper_daw_bridge.{}",
        env::consts::DLL_PREFIX,
        env::consts::DLL_EXTENSION
    );
    let mut candidates = Vec::new();
    if let Ok(exe) = env::current_exe()
        && let Some(profile_dir) = exe.parent()
    {
        candidates.push(profile_dir.join(&lib));
        if let Some(target_dir) = profile_dir.parent() {
            candidates.push(target_dir.join("debug").join(&lib));
            candidates.push(target_dir.join("release").join(&lib));
        }
    }
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("target/debug").join(&lib));
        candidates.push(cwd.join("target/release").join(&lib));
    }
    candidates.into_iter().find(|path| path.is_file())
}

pub fn spawn_reaper(profile: &DawProfile) -> Result<u32> {
    spawn_reaper_with_env(profile, &[])
}

/// Like [`spawn_reaper`] but lets the caller inject extra env vars into the
/// REAPER process (e.g. `FTS_SYNC_ENABLED=1` to enable the daw-bridge sync
/// runtime).
pub fn spawn_reaper_with_env(profile: &DawProfile, extra_env: &[(&str, &str)]) -> Result<u32> {
    ensure_reaper_profile_dirs(profile)?;

    let mut cmd = if profile.sandboxed {
        let launcher = sandbox_launcher().ok_or_else(|| {
            eyre::eyre!(
                "Profile '{}' is sandboxed, but no sandbox launcher was found. Set FTS_REAPER_SANDBOX or install reaper-env/fts-test.",
                profile.id
            )
        })?;
        let mut cmd = Command::new(launcher);
        cmd.arg(&profile.executable);
        cmd
    } else {
        Command::new(&profile.executable)
    };

    cmd.current_dir(&profile.resources_dir)
        .env("FTS_DAW_PROFILE", profile.id)
        .env("FTS_DAW_ROLE", profile.role)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg("-cfgfile")
        .arg(&profile.ini_path)
        .arg("-newinst")
        .arg("-nosplash")
        .arg("-ignoreerrors");

    for (k, v) in extra_env {
        cmd.env(k, v);
    }

    let child = cmd.spawn().map_err(|e| {
        eyre::eyre!(
            "Failed to spawn REAPER ({}) at {}: {e}",
            profile.label,
            profile.executable
        )
    })?;

    let pid = child.id();
    drop(child);
    Ok(pid)
}

pub fn kill_reaper(pid: u32) -> bool {
    let process_group = format!("-{pid}");
    if Command::new("kill")
        .args(["-TERM", "--", &process_group])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return true;
    }

    Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Spawn REAPER and wait for its Unix socket to appear, then connect.
///
/// Returns `(Daw, pid, socket_path)` on success. The caller is responsible
/// for calling `teardown_owned(pid, socket_path)` when done.
pub async fn launch_and_connect(config_id: &str) -> Result<(DawConnection, u32, PathBuf)> {
    let profile =
        profile_by_id(config_id).ok_or_else(|| eyre::eyre!("Unknown DAW profile: {config_id}"))?;

    eprintln!("Spawning REAPER ({})...", profile.label);
    let before = discover_all_sockets()
        .into_iter()
        .map(|(_, path)| path)
        .collect::<BTreeSet<_>>();
    let pid = spawn_reaper(&profile)?;

    // Wait up to 30s for socket to appear
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    eprint!("  Waiting for socket");
    let socket_path;
    loop {
        if let Some(path) = discover_all_sockets()
            .into_iter()
            .map(|(_, path)| path)
            .find(|path| !before.contains(path))
        {
            socket_path = path;
            break;
        }
        if std::time::Instant::now() > deadline {
            eprintln!();
            kill_reaper(pid);
            return Err(eyre::eyre!("Timed out waiting for REAPER socket after 30s"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        eprint!(".");
    }
    eprintln!("\n  Socket ready: {}", socket_path.display());

    let daw = connect(Some(socket_path.clone())).await?;
    Ok((daw, pid, socket_path))
}

/// Kill an owned REAPER instance and remove its socket file.
pub fn teardown_owned(pid: u32, socket: &PathBuf) {
    kill_reaper(pid);
    let _ = std::fs::remove_file(socket);
    eprintln!("REAPER (PID {pid}) stopped.");
}

// ============================================================================
// Connection
// ============================================================================

pub async fn connect(socket: Option<PathBuf>) -> Result<DawConnection> {
    let path = match socket {
        Some(p) => p,
        None => discover_socket().ok_or_else(|| {
            eyre::eyre!("No DAW socket found in /tmp. Is REAPER running with the FTS extension?")
        })?,
    };

    eprintln!("Connecting to {}", path.display());

    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::UnixStream::connect(&path),
    )
    .await
    .map_err(|_| eyre::eyre!("Timed out connecting to {}", path.display()))?
    .map_err(|e| eyre::eyre!("Failed to connect to {}: {}", path.display(), e))?;

    let link = vox_stream::StreamLink::unix(stream);

    // vox 0.10 lane model: establish the connection, then open the DAW
    // service lane (carries `vox-service: daw-cli` automatically) which
    // yields a ready-to-use `Caller`.
    let connection = vox::initiator_on(link)
        .establish_connection()
        .await
        .map_err(|e| eyre::eyre!("Failed to establish vox connection: {:?}", e))?;
    let client = connection
        .open_lane::<DawLaneClient>()
        .await
        .map_err(|e| eyre::eyre!("Failed to open DAW lane: {:?}", e))?;

    Ok(DawConnection {
        daw: Daw::new(client.caller),
        _connection: connection,
    })
}

// ============================================================================
// Track Resolution
// ============================================================================

/// Parse a track argument as either an index (if numeric) or name.
/// Returns (guid, name).
pub async fn resolve_track(daw: &Daw, track_arg: &str) -> Result<(String, String)> {
    let project = daw.current_project().await?;
    let tracks = project.tracks();

    // Try as index first
    if let Ok(idx) = track_arg.parse::<u32>() {
        if let Some(handle) = tracks.by_index(idx).await? {
            let info = handle.info().await?;
            return Ok((info.guid.clone(), info.name.clone()));
        }
        bail!("No track at index {idx}");
    }

    // Try as name
    if let Some(handle) = tracks.by_name(track_arg).await? {
        let info = handle.info().await?;
        return Ok((info.guid.clone(), info.name.clone()));
    }

    bail!("No track named \"{track_arg}\"");
}

/// Resolve a track argument and return the TrackHandle directly.
pub async fn resolve_track_handle(daw: &Daw, track_arg: &str) -> Result<crate::rpc::TrackHandle> {
    let (guid, _) = resolve_track(daw, track_arg).await?;
    let project = daw.current_project().await?;
    project
        .tracks()
        .by_guid(&guid)
        .await?
        .ok_or_else(|| eyre::eyre!("Track not found"))
}

/// Resolve an FX argument (index or name) on a track's FX chain.
pub async fn resolve_fx_handle(
    fx_chain: &crate::rpc::FxChain,
    fx_arg: &str,
    track_name: &str,
) -> Result<crate::rpc::FxHandle> {
    let fx_handle = if let Ok(idx) = fx_arg.parse::<u32>() {
        fx_chain.by_index(idx).await?
    } else {
        fx_chain.by_name(fx_arg).await?
    };
    fx_handle.ok_or_else(|| eyre::eyre!("FX \"{fx_arg}\" not found on track \"{track_name}\""))
}

// ============================================================================
// Formatting Helpers
// ============================================================================

pub fn format_position(pos: &crate::service::primitives::Position) -> String {
    if let Some(ref musical) = pos.musical {
        format!(
            "{}.{}.{:03}",
            musical.measure, musical.beat, musical.subdivision
        )
    } else if let Some(ref time) = pos.time {
        let secs = time.as_seconds();
        let mins = (secs / 60.0).floor() as u32;
        let remaining = secs - (mins as f64 * 60.0);
        format!("{}:{:06.3}", mins, remaining)
    } else {
        "?".to_string()
    }
}

pub fn vol_to_db(vol: f64) -> String {
    if vol <= 0.0 {
        "-inf dB".to_string()
    } else {
        format!("{:.1} dB", 20.0 * vol.log10())
    }
}

pub fn pan_to_string(pan: f64) -> String {
    if pan.abs() < 0.005 {
        "C".to_string()
    } else if pan < 0.0 {
        format!("{:.0}%L", pan.abs() * 100.0)
    } else {
        format!("{:.0}%R", pan * 100.0)
    }
}

pub fn fx_type_str(ft: &FxType) -> &'static str {
    match ft {
        FxType::Vst2 => "VST2",
        FxType::Vst3 => "VST3",
        FxType::Au => "AU",
        FxType::Js => "JS",
        FxType::Clap => "CLAP",
        FxType::Unknown => "?",
    }
}

pub fn flags_str(muted: bool, soloed: bool, armed: bool) -> String {
    let mut flags = Vec::new();
    if muted {
        flags.push("M");
    }
    if soloed {
        flags.push("S");
    }
    if armed {
        flags.push("R");
    }
    if flags.is_empty() {
        "-".to_string()
    } else {
        flags.join("")
    }
}

// ============================================================================
// Commands
// ============================================================================

pub async fn cmd_info(daw: &Daw, as_json: bool) -> Result<()> {
    let project = daw.current_project().await?;
    let info = project.info().await?;
    let track_count = project.n_tracks().await?;
    let transport = project.transport().get_state().await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "name": info.name,
                "path": info.path,
                "guid": info.guid,
                "track_count": track_count,
                "tempo": transport.tempo.bpm,
                "time_signature": format!("{}/{}", transport.time_signature.numerator, transport.time_signature.denominator),
            }))?
        );
    } else {
        println!("Project: {}", info.name);
        println!("Path:    {}", info.path);
        println!("GUID:    {}", info.guid);
        println!("Tracks:  {}", track_count);
        println!("Tempo:   {:.1} BPM", transport.tempo.bpm);
        println!(
            "Time:    {}/{}",
            transport.time_signature.numerator, transport.time_signature.denominator
        );
    }
    Ok(())
}

pub async fn cmd_tracks(daw: &Daw, as_json: bool) -> Result<()> {
    let project = daw.current_project().await?;
    let all_tracks = project.tracks().all().await?;

    if as_json {
        let arr: Vec<_> = all_tracks
            .iter()
            .map(|t| {
                json!({
                    "index": t.index,
                    "name": t.name,
                    "guid": t.guid,
                    "muted": t.muted,
                    "soloed": t.soloed,
                    "armed": t.armed,
                    "selected": t.selected,
                    "volume": t.volume,
                    "volume_db": vol_to_db(t.volume),
                    "pan": t.pan,
                    "is_folder": t.is_folder,
                    "folder_depth": t.folder_depth,
                    "fx_count": t.fx_count,
                    "input_fx_count": t.input_fx_count,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if all_tracks.is_empty() {
            println!("No tracks.");
            return Ok(());
        }

        // Header
        println!(
            "{:>4}  {:<30}  {:>5}  {:>9}  {:>5}  {:>3}",
            "#", "Name", "Flags", "Volume", "Pan", "FX"
        );
        println!("{}", "-".repeat(68));

        for t in &all_tracks {
            let indent = if t.folder_depth > 0 {
                "  ".repeat(t.folder_depth as usize)
            } else {
                String::new()
            };
            let name = format!("{}{}{}", indent, if t.is_folder { "[" } else { "" }, t.name,);
            let name = if t.is_folder {
                format!("{}]", name)
            } else {
                name
            };
            println!(
                "{:>4}  {:<30}  {:>5}  {:>9}  {:>5}  {:>3}",
                t.index,
                if name.len() > 30 { &name[..30] } else { &name },
                flags_str(t.muted, t.soloed, t.armed),
                vol_to_db(t.volume),
                pan_to_string(t.pan),
                t.fx_count,
            );
        }
    }
    Ok(())
}

pub async fn cmd_track(daw: &Daw, track_arg: &str, as_json: bool) -> Result<()> {
    let handle = resolve_track_handle(daw, track_arg).await?;
    let t = handle.info().await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "index": t.index,
                "name": t.name,
                "guid": t.guid,
                "muted": t.muted,
                "soloed": t.soloed,
                "armed": t.armed,
                "selected": t.selected,
                "volume": t.volume,
                "volume_db": vol_to_db(t.volume),
                "pan": t.pan,
                "is_folder": t.is_folder,
                "folder_depth": t.folder_depth,
                "parent_guid": t.parent_guid,
                "visible_in_tcp": t.visible_in_tcp,
                "visible_in_mixer": t.visible_in_mixer,
                "fx_count": t.fx_count,
                "input_fx_count": t.input_fx_count,
                "color": t.color,
            }))?
        );
    } else {
        println!("Track #{}: {}", t.index, t.name);
        println!("  GUID:     {}", t.guid);
        println!("  Volume:   {} ({:.4})", vol_to_db(t.volume), t.volume);
        println!("  Pan:      {}", pan_to_string(t.pan));
        println!("  Muted:    {}", t.muted);
        println!("  Soloed:   {}", t.soloed);
        println!("  Armed:    {}", t.armed);
        println!("  Selected: {}", t.selected);
        println!("  Folder:   {}", t.is_folder);
        if let Some(ref parent) = t.parent_guid {
            println!("  Parent:   {}", parent);
        }
        println!("  FX:       {} (input: {})", t.fx_count, t.input_fx_count);
        if let Some(color) = t.color {
            println!("  Color:    #{:06X}", color);
        }
    }
    Ok(())
}

pub async fn cmd_fx(daw: &Daw, track_arg: &str, as_json: bool) -> Result<()> {
    let (guid, track_name) = resolve_track(daw, track_arg).await?;
    let project = daw.current_project().await?;
    let handle = project
        .tracks()
        .by_guid(&guid)
        .await?
        .ok_or_else(|| eyre::eyre!("Track not found"))?;
    let fx_chain = handle.fx_chain();
    let fx_list = fx_chain.all().await?;

    if as_json {
        let arr: Vec<_> = fx_list
            .iter()
            .map(|f| {
                json!({
                    "index": f.index,
                    "name": f.name,
                    "plugin_name": f.plugin_name,
                    "plugin_type": fx_type_str(&f.plugin_type),
                    "guid": f.guid,
                    "enabled": f.enabled,
                    "offline": f.offline,
                    "parameter_count": f.parameter_count,
                    "preset_name": f.preset_name,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if fx_list.is_empty() {
            println!("No FX on track \"{}\".", track_name);
            return Ok(());
        }
        println!(
            "FX chain for \"{}\" ({} plugins):",
            track_name,
            fx_list.len()
        );
        println!();
        for f in &fx_list {
            let status = if !f.enabled {
                " [BYPASS]"
            } else if f.offline {
                " [OFFLINE]"
            } else {
                ""
            };
            println!(
                "  {:>2}. {} ({}){}",
                f.index,
                f.name,
                fx_type_str(&f.plugin_type),
                status,
            );
            if let Some(ref preset) = f.preset_name {
                println!("      Preset: {}", preset);
            }
            println!("      Params: {}  GUID: {}", f.parameter_count, f.guid);
        }
    }
    Ok(())
}

pub async fn cmd_plugins(daw: &Daw, as_json: bool) -> Result<()> {
    let plugins = daw.installed_plugins().await?;

    if as_json {
        let arr: Vec<_> = plugins
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "ident": p.ident,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        println!("{:<4} {:<50} Identifier", "#", "Name");
        println!("{}", "\u{2500}".repeat(100));
        for (i, p) in plugins.iter().enumerate() {
            println!("{:<4} {:<50} {}", i, p.name, p.ident);
        }
        println!("\n{} plugins installed", plugins.len());
    }
    Ok(())
}

pub async fn cmd_params(daw: &Daw, track_arg: &str, fx_arg: &str, as_json: bool) -> Result<()> {
    let (_, track_name) = resolve_track(daw, track_arg).await?;
    let handle = resolve_track_handle(daw, track_arg).await?;
    let fx_chain = handle.fx_chain();
    let fx_handle = resolve_fx_handle(&fx_chain, fx_arg, &track_name).await?;
    let fx_info = fx_handle.info().await?;
    let params = fx_handle.parameters().await?;

    if as_json {
        let arr: Vec<_> = params
            .iter()
            .map(|p| {
                let mut obj = json!({
                    "index": p.index,
                    "name": p.name,
                    "value": p.value,
                    "formatted": p.formatted,
                    "is_toggle": p.is_toggle,
                });
                if let Some(steps) = p.step_count {
                    obj["step_count"] = json!(steps);
                }
                if !p.step_labels.is_empty() {
                    obj["step_labels"] = json!(
                        p.step_labels
                            .iter()
                            .map(|(v, l)| json!({"value": v, "label": l}))
                            .collect::<Vec<_>>()
                    );
                }
                obj
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        println!(
            "Parameters for \"{}\" on \"{}\" ({} params):",
            fx_info.name,
            track_name,
            params.len()
        );
        println!();
        println!("{:>4}  {:<35}  {:>8}  Display", "#", "Name", "Value");
        println!("{}", "-".repeat(75));

        for p in &params {
            println!(
                "{:>4}  {:<35}  {:>8.4}  {}",
                p.index,
                if p.name.len() > 35 {
                    &p.name[..35]
                } else {
                    &p.name
                },
                p.value,
                p.formatted,
            );
        }
    }
    Ok(())
}

pub async fn cmd_transport(daw: &Daw, as_json: bool) -> Result<()> {
    let project = daw.current_project().await?;
    let state = project.transport().get_state().await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "play_state": format!("{:?}", state.play_state),
                "record_mode": format!("{:?}", state.record_mode),
                "looping": state.looping,
                "tempo": state.tempo.bpm,
                "playrate": state.playrate,
                "time_signature": format!("{}/{}", state.time_signature.numerator, state.time_signature.denominator),
                "playhead": format_position(&state.playhead_position),
                "edit_cursor": format_position(&state.edit_position),
            }))?
        );
    } else {
        println!("Transport");
        println!("  State:     {:?}", state.play_state);
        println!("  Playhead:  {}", format_position(&state.playhead_position));
        println!("  Edit:      {}", format_position(&state.edit_position));
        println!("  Tempo:     {:.1} BPM", state.tempo.bpm);
        println!(
            "  Time Sig:  {}/{}",
            state.time_signature.numerator, state.time_signature.denominator
        );
        println!("  Playrate:  {:.2}x", state.playrate);
        println!("  Looping:   {}", state.looping);
        if let Some(ref lr) = state.loop_region {
            println!(
                "  Loop:      {:.3}s - {:.3}s",
                lr.start_seconds, lr.end_seconds
            );
        }
        println!("  Record:    {:?}", state.record_mode);
    }
    Ok(())
}

pub async fn cmd_markers(daw: &Daw, as_json: bool) -> Result<()> {
    let project = daw.current_project().await?;
    let markers = project.markers().all().await?;

    if as_json {
        let arr: Vec<_> = markers
            .iter()
            .map(|m| {
                json!({
                    "id": m.id,
                    "name": m.name,
                    "position": format_position(&m.position),
                    "color": m.color,
                    "guid": m.guid,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if markers.is_empty() {
            println!("No markers.");
            return Ok(());
        }
        println!("{:>4}  {:<14}  Name", "ID", "Position");
        println!("{}", "-".repeat(45));
        for m in &markers {
            println!(
                "{:>4}  {:<14}  {}",
                m.id.map(|i| i.to_string()).unwrap_or_default(),
                format_position(&m.position),
                m.name,
            );
        }
    }
    Ok(())
}

pub async fn cmd_regions(daw: &Daw, as_json: bool) -> Result<()> {
    let project = daw.current_project().await?;
    let regions = project.regions().all().await?;

    if as_json {
        let arr: Vec<_> = regions
            .iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "name": r.name,
                    "start": format_position(&r.time_range.start),
                    "end": format_position(&r.time_range.end),
                    "color": r.color,
                    "guid": r.guid,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if regions.is_empty() {
            println!("No regions.");
            return Ok(());
        }
        println!("{:>4}  {:<14}  {:<14}  Name", "ID", "Start", "End");
        println!("{}", "-".repeat(55));
        for r in &regions {
            println!(
                "{:>4}  {:<14}  {:<14}  {}",
                r.id.map(|i| i.to_string()).unwrap_or_default(),
                format_position(&r.time_range.start),
                format_position(&r.time_range.end),
                r.name,
            );
        }
    }
    Ok(())
}

pub async fn cmd_ping(daw: &Daw) -> Result<()> {
    if daw.healthcheck().await {
        println!("OK");
    } else {
        bail!("Health check failed");
    }
    Ok(())
}

// ============================================================================
// Process & Project Management Commands
// ============================================================================

pub fn cmd_launch(config_id: Option<&str>) -> Result<()> {
    let id = config_id.unwrap_or("fasttrackstudio");
    let profile = profile_by_id(id).ok_or_else(|| {
        let known: Vec<_> = daw_profiles().iter().map(|c| c.id).collect();
        eyre::eyre!(
            "Unknown DAW profile \"{id}\". Known profiles: {}",
            known.join(", ")
        )
    })?;

    let pid = spawn_reaper(&profile)?;
    println!("Launched {} (PID {pid})", profile.label);
    Ok(())
}

pub fn profiles_value() -> serde_json::Value {
    json!(
        daw_profiles()
            .into_iter()
            .map(|profile| {
                json!({
                    "id": profile.id,
                    "label": profile.label,
                    "daw": profile.daw,
                    "role": profile.role,
                    "sandboxed": profile.sandboxed,
                    "executable": profile.executable,
                    "resources_dir": profile.resources_dir,
                    "ini_path": profile.ini_path,
                })
            })
            .collect::<Vec<_>>()
    )
}

pub fn cmd_profiles(as_json: bool) -> Result<()> {
    let profiles = profiles_value();
    if as_json {
        println!("{}", serde_json::to_string_pretty(&profiles)?);
        return Ok(());
    }

    println!(
        "{:<18} {:<8} {:<10} {:<9} CONFIG",
        "PROFILE", "DAW", "ROLE", "SANDBOX"
    );
    for profile in profiles.as_array().into_iter().flatten() {
        println!(
            "{:<18} {:<8} {:<10} {:<9} {}",
            profile["id"].as_str().unwrap_or_default(),
            profile["daw"].as_str().unwrap_or_default(),
            profile["role"].as_str().unwrap_or_default(),
            if profile["sandboxed"].as_bool().unwrap_or(false) {
                "yes"
            } else {
                "no"
            },
            profile["resources_dir"].as_str().unwrap_or_default(),
        );
    }
    Ok(())
}

pub fn cmd_quit(pid: Option<u32>) -> Result<()> {
    let target_pid = match pid {
        Some(p) => p,
        None => {
            // Extract PID from the discovered socket
            let sockets = discover_all_sockets();
            if sockets.is_empty() {
                bail!("No running DAW instances found");
            }
            if sockets.len() > 1 {
                eprintln!("Multiple instances found:");
                for (pid, path) in &sockets {
                    eprintln!("  PID {pid}  {}", path.display());
                }
                eprintln!("Killing most recent (PID {})", sockets[0].0);
            }
            sockets[0].0
        }
    };

    if kill_reaper(target_pid) {
        for _ in 0..8 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            clean_stale_daw_sockets();
        }
        println!("Sent SIGTERM to PID {target_pid}");
    } else {
        bail!("Failed to kill PID {target_pid}");
    }
    Ok(())
}

pub async fn cmd_projects(daw: &Daw, as_json: bool) -> Result<()> {
    let projects = daw.projects().await?;

    if as_json {
        let mut arr = Vec::new();
        for (i, p) in projects.iter().enumerate() {
            let info = p.info().await?;
            arr.push(json!({
                "index": i,
                "name": info.name,
                "guid": info.guid,
                "path": info.path,
            }));
        }
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        if projects.is_empty() {
            println!("No open projects.");
            return Ok(());
        }
        println!("{:>3}  {:<30}  {:<38}  Path", "#", "Name", "GUID");
        println!("{}", "-".repeat(100));
        for (i, p) in projects.iter().enumerate() {
            let info = p.info().await?;
            println!(
                "{:>3}  {:<30}  {:<38}  {}",
                i,
                if info.name.len() > 30 {
                    &info.name[..30]
                } else {
                    &info.name
                },
                info.guid,
                info.path,
            );
        }
    }
    Ok(())
}

pub async fn cmd_open(daw: &Daw, path: &str, as_json: bool) -> Result<()> {
    let project = daw.open_project(path).await?;
    let info = project.info().await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "name": info.name,
                "guid": info.guid,
                "path": info.path,
            }))?
        );
    } else {
        println!("Opened project: {}", info.name);
        println!("  GUID: {}", info.guid);
        println!("  Path: {}", info.path);
    }
    Ok(())
}

pub async fn cmd_close(daw: &Daw, guid: Option<&str>) -> Result<()> {
    let target_guid = match guid {
        Some(g) => g.to_string(),
        None => {
            let project = daw.current_project().await?;
            let info = project.info().await?;
            info.guid.clone()
        }
    };

    daw.close_project(&target_guid).await?;
    println!("Closed project {target_guid}");
    Ok(())
}

pub async fn cmd_add_track(
    daw: &Daw,
    name: Option<&str>,
    at_index: Option<u32>,
    as_json: bool,
) -> Result<()> {
    let project = daw.current_project().await?;
    let track_name = name.unwrap_or("New Track");
    let handle = project.tracks().add(track_name, at_index).await?;
    let info = handle.info().await?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "index": info.index,
                "name": info.name,
                "guid": info.guid,
            }))?
        );
    } else {
        println!(
            "Added track #{}: {} (GUID: {})",
            info.index, info.name, info.guid
        );
    }
    Ok(())
}

pub async fn cmd_remove_track(daw: &Daw, track_arg: &str) -> Result<()> {
    let (guid, name) = resolve_track(daw, track_arg).await?;
    let project = daw.current_project().await?;
    project
        .tracks()
        .remove(crate::service::TrackRef::Guid(guid.clone()))
        .await?;
    println!("Removed track \"{}\" ({})", name, guid);
    Ok(())
}

// ── Items / Takes ────────────────────────────────────────────────────────────

pub async fn cmd_items(daw: &Daw, track_arg: &str, as_json: bool) -> Result<()> {
    let handle = resolve_track_handle(daw, track_arg).await?;
    let items = handle.items().all().await?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &items
                    .iter()
                    .map(|i| json!({
                        "guid": i.guid,
                        "position": i.position.as_seconds(),
                        "length": i.length.as_seconds(),
                        "muted": i.muted,
                        "selected": i.selected,
                        "take_count": i.take_count,
                        "active_take": i.active_take_index,
                    }))
                    .collect::<Vec<_>>()
            )?
        );
    } else if items.is_empty() {
        println!("(no items on track)");
    } else {
        println!(
            "{:<3} {:>10} {:>10}  {:<6} {:<3}  GUID",
            "#", "pos", "len", "takes", "sel"
        );
        for (idx, it) in items.iter().enumerate() {
            println!(
                "{:<3} {:>10.3} {:>10.3}  {:<6} {:<3}  {}",
                idx,
                it.position.as_seconds(),
                it.length.as_seconds(),
                it.take_count,
                if it.selected { "yes" } else { "no" },
                it.guid,
            );
        }
    }
    Ok(())
}

pub async fn cmd_takes(daw: &Daw, track_arg: &str, item_idx: u32, as_json: bool) -> Result<()> {
    let track = resolve_track_handle(daw, track_arg).await?;
    let item = track
        .items()
        .by_index(item_idx)
        .await?
        .ok_or_else(|| eyre::eyre!("item #{} not found on track", item_idx))?;
    let info = item.info().await?;
    let takes = item.takes().all().await?;

    let mut rows = Vec::new();
    for (idx, take) in takes.iter().enumerate() {
        let handle = item
            .takes()
            .by_index(idx as u32)
            .await?
            .ok_or_else(|| eyre::eyre!("take #{} disappeared mid-call", idx))?;
        let source_type = handle.source_type().await?;
        rows.push(json!({
            "index": idx,
            "guid": take.guid,
            "name": take.name,
            "is_active": idx as u32 == info.active_take_index,
            "source_type": format!("{:?}", source_type),
            "is_midi": take.is_midi,
            "midi_note_count": take.midi_note_count,
            "pitch": take.pitch,
            "play_rate": take.play_rate,
            "preserve_pitch": take.preserve_pitch,
            "color": take.color,
        }));
    }

    if as_json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else if rows.is_empty() {
        println!("(no takes on item)");
    } else {
        println!(
            "{:<3} {:<6} {:<6} {:<7} {:<5}  name",
            "#", "act", "type", "midi#", "rate"
        );
        for row in &rows {
            println!(
                "{:<3} {:<6} {:<6} {:<7} {:<5.3}  {}",
                row["index"],
                if row["is_active"].as_bool().unwrap_or(false) {
                    "*"
                } else {
                    ""
                },
                row["source_type"].as_str().unwrap_or("?"),
                row["midi_note_count"]
                    .as_u64()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "-".into()),
                row["play_rate"].as_f64().unwrap_or(1.0),
                row["name"].as_str().unwrap_or(""),
            );
        }
    }
    Ok(())
}

pub async fn cmd_take_delete(
    daw: &Daw,
    track_arg: &str,
    item_idx: u32,
    take_idx: u32,
) -> Result<()> {
    let take = resolve_take(daw, track_arg, item_idx, take_idx).await?;
    take.delete().await?;
    println!("Deleted take #{} on item #{}", take_idx, item_idx);
    Ok(())
}

pub async fn cmd_take_preserve_pitch(
    daw: &Daw,
    track_arg: &str,
    item_idx: u32,
    take_idx: u32,
    preserve: bool,
) -> Result<()> {
    let take = resolve_take(daw, track_arg, item_idx, take_idx).await?;
    take.set_preserve_pitch(preserve).await?;
    println!(
        "Take #{}: preserve_pitch = {}",
        take_idx,
        if preserve { "on" } else { "off" }
    );
    Ok(())
}

pub async fn cmd_take_set_source(
    daw: &Daw,
    track_arg: &str,
    item_idx: u32,
    take_idx: u32,
    path: &str,
) -> Result<()> {
    let take = resolve_take(daw, track_arg, item_idx, take_idx).await?;
    take.set_source_file(path).await?;
    println!("Take #{}: source set to {}", take_idx, path);
    Ok(())
}

async fn resolve_take(
    daw: &Daw,
    track_arg: &str,
    item_idx: u32,
    take_idx: u32,
) -> Result<crate::rpc::TakeHandle> {
    let track = resolve_track_handle(daw, track_arg).await?;
    let item = track
        .items()
        .by_index(item_idx)
        .await?
        .ok_or_else(|| eyre::eyre!("item #{} not found on track", item_idx))?;
    item.takes()
        .by_index(take_idx)
        .await?
        .ok_or_else(|| eyre::eyre!("take #{} not found on item #{}", take_idx, item_idx))
}

// ── File Operations ──────────────────────────────────────────────────────────

// r[impl cli.combine]
pub async fn cmd_combine(
    daw: &Daw,
    input: &str,
    output: Option<&str>,
    gap_measures: u32,
) -> Result<()> {
    let summary = ops::combine_rpl(daw, input, output, gap_measures).await?;
    let song_count = summary["song_count"].as_u64().unwrap_or(0);
    let output_path = summary["output"].as_str().unwrap_or("<unknown>");

    // Print summary
    println!("Combined {} songs → {}", song_count, output_path);
    if gap_measures > 0 {
        println!("Gap: {} measure(s) between songs", gap_measures);
    }
    println!();
    let mut total: f64 = 0.0;
    for song in summary["songs"].as_array().into_iter().flatten() {
        let index = song["index"].as_u64().unwrap_or(0);
        let name = song["name"].as_str().unwrap_or("<unnamed>");
        let global_start_seconds = song["global_start_seconds"].as_f64().unwrap_or(0.0);
        let duration_seconds = song["duration_seconds"].as_f64().unwrap_or(0.0);
        println!(
            "  {:>2}. {:<40} {:>6.1}s ({:.0}:{:02.0})",
            index,
            name,
            global_start_seconds,
            (duration_seconds / 60.0).floor(),
            duration_seconds % 60.0,
        );
        total = global_start_seconds + duration_seconds;
    }
    println!();
    println!("Total: {:.0}:{:02.0}", (total / 60.0).floor(), total % 60.0,);

    Ok(())
}

#[cfg(test)]
mod launcher_tests {
    use super::*;

    fn temp_profile(name: &str) -> DawProfile {
        let root = std::env::temp_dir().join(format!("daw-cli-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let resources_dir = root.join("Reaper");
        DawProfile {
            id: "test",
            label: "Test REAPER",
            daw: "reaper",
            executable: "reaper".to_string(),
            ini_path: resources_dir.join("reaper.ini"),
            resources_dir,
            role: "test",
            sandboxed: false,
        }
    }

    #[test]
    fn bootstrap_reaper_toolbars_only_adds_missing_sections() {
        let profile = temp_profile("toolbar-bootstrap");
        fs::create_dir_all(&profile.resources_dir).unwrap();
        let menu_path = profile.resources_dir.join("reaper-menu.ini");
        fs::write(
            &menu_path,
            "[Main toolbar]\nitem_0=40023 New project...\n\n[Floating toolbar 1]\nitem_0=_FTS_EXISTING Existing\n\n[Floating MIDI toolbar 1]\nitem_0=_FTS_MIDI Existing MIDI\n",
        )
        .unwrap();

        bootstrap_reaper_toolbars(&profile).unwrap();
        let content = fs::read_to_string(&menu_path).unwrap();

        assert!(content.contains("[Floating toolbar 1]\nitem_0=_FTS_EXISTING Existing\n"));
        assert!(content.contains("[Floating toolbar 2]\nitem_0=41101 Edit me\n"));
        assert!(content.contains("[Floating toolbar 32]\nitem_0=41101 Edit me\n"));
        assert!(content.contains("[Floating MIDI toolbar 1]\nitem_0=_FTS_MIDI Existing MIDI\n"));
        assert!(content.contains("[Floating MIDI toolbar 2]\nitem_0=41101 Edit me\n"));
        assert!(content.contains("[Floating MIDI toolbar 8]\nitem_0=41101 Edit me\n"));
        assert_eq!(content.matches("[Floating toolbar 1]").count(), 1);
        assert_eq!(content.matches("[Floating MIDI toolbar 1]").count(), 1);

        let _ = fs::remove_dir_all(profile.resources_dir.parent().unwrap());
    }
}
