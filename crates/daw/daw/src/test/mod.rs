//! DAW integration-test harness (feature `test-harness`).
//!
//! Spawns a DAW backend, drives it over the `daw` RPC surface, and runs
//! `#[daw_test]` / `#[reaper_test]` bodies. The backend is abstracted behind
//! the [`DawBackend`] trait; [`ReaperBackend`] is the single concrete impl.
//!
//! Canonical (DAW-generic) names — with source-compat aliases:
//! - [`DawTestContext`] (alias [`ReaperTestContext`])
//! - [`run_daw_test`] (alias [`run_reaper_test`])
//! - [`run_multi_daw_test`] (alias [`run_multi_reaper_test`])
//! - [`DawBackend`] / [`ReaperBackend`]
//!
//! Provides:
//! - [`DawTestContext`] — wraps shared `Daw` + project handle + per-test logging
//! - [`run_daw_test`] — assigns a project tab, runs test, cleans up
//! - [`ReaperProcess`] — spawn/wait/kill-on-drop guard
//! - [`connect_daw`] — connection + polling logic
//! - Re-exports `#[daw_test]` / `#[reaper_test]` from `daw-test-macro`

pub mod display;
pub use display::{Recording, VirtualDisplay};

pub mod runner;

use crate::rpc::{Daw, Project, TrackHandle};
use eyre::Result;
use std::{
    fs::{self, File},
    future::Future,
    io::Write,
    mem::ManuallyDrop,
    path::{Path, PathBuf},
    pin::Pin,
    process::{Child, Command},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::runtime::Runtime;

// Re-export the proc-macros so users can `use daw::test::{daw_test, reaper_test};`
pub use daw_test_macro::{daw_test, reaper_test};

// ─────────────────────────────────────────────────────────────
//  Constants
// ─────────────────────────────────────────────────────────────

/// Default socket path prefix. Each REAPER instance creates
/// `/tmp/fts-daw-{pid}.sock`. Use `FTS_SOCKET` env var to override.
pub const SOCKET_PREFIX: &str = "/tmp/fts-daw-";
pub const SOCKET_SUFFIX: &str = ".sock";

/// Legacy constant kept for reference — actual path is now PID-based.
#[deprecated(note = "Use SOCKET_PREFIX + PID + SOCKET_SUFFIX instead")]
pub const SOCKET_PATH: &str = "/tmp/fts-control.sock";
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const REAPER_BOOT_TIMEOUT_SECS: u64 = 30;
/// Resolve the FTS home directory.
///
/// Checks `$FTS_HOME`, then falls back to `~/fasttrackstudio`.
fn fts_home() -> String {
    if let Ok(p) = std::env::var("FTS_HOME") {
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    format!("{home}/fasttrackstudio")
}

/// Resolve the REAPER executable path.
///
/// Checks `$FTS_REAPER_EXECUTABLE`, then `reaper` on `$PATH` (Linux),
/// then falls back to `<fts_home>/Reaper/FTS-TESTING.app/Contents/MacOS/REAPER` (macOS).
pub fn reaper_executable() -> String {
    if let Ok(exe) = std::env::var("FTS_REAPER_EXECUTABLE") {
        return exe;
    }

    // On Linux, prefer `reaper` on PATH (provided by nix/devenv)
    if cfg!(target_os = "linux")
        && let Some(exe) = which_command("reaper")
    {
        return exe;
    }

    // macOS .app bundle fallback
    let fts = fts_home();
    let production = format!("{fts}/Reaper/FTS-TRACKS/FTS-TESTING.app/Contents/MacOS/REAPER");
    if std::path::Path::new(&production).exists() {
        return production;
    }
    format!("{fts}/Reaper/FTS-TESTING.app/Contents/MacOS/REAPER")
}

/// Resolve the REAPER resources path.
///
/// Checks `$FTS_REAPER_RESOURCES`, then `$FTS_REAPER_CONFIG`,
/// then `~/fasttrackstudio` (Linux — the real install; `~/fts-dev` is
/// the dev-iteration copy and `~/.config/FastTrackStudio/Reaper` is a
/// compat symlink to `~/fasttrackstudio`),
/// then falls back to macOS `.app` bundle paths.
pub fn reaper_resources() -> String {
    if let Ok(p) = std::env::var("FTS_REAPER_RESOURCES") {
        return p;
    }

    // On Linux, use the canonical install directory
    if cfg!(target_os = "linux") {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        if let Ok(p) = std::env::var("FTS_REAPER_CONFIG") {
            return p.replace("$HOME", &home);
        }
        return format!("{home}/fasttrackstudio");
    }

    // macOS .app bundle fallback
    let fts = fts_home();
    let production = format!("{fts}/Reaper/FTS-TRACKS/FTS-TESTING.app/Contents/Resources");
    if std::path::Path::new(&production).exists() {
        return production;
    }
    format!("{fts}/Reaper/FTS-TESTING.app/Contents/Resources")
}
pub const LOG_DIR: &str = "/tmp/reaper-tests";

/// Per-test log directory. `/tmp/reaper-tests` is shared between users on
/// one host (CI runs as gitea-runner beside dev runs as the login user),
/// and a dir created by one user is unwritable for the other — so prefer
/// `FTS_TEST_LOG_DIR`, then `<FTS_REAPER_RESOURCES>/test-logs` (the rig
/// is always run-owned), falling back to the legacy shared path.
pub fn log_dir() -> PathBuf {
    if let Ok(d) = std::env::var("FTS_TEST_LOG_DIR") {
        return PathBuf::from(d);
    }
    if let Ok(rig) = std::env::var("FTS_REAPER_RESOURCES") {
        return PathBuf::from(rig).join("test-logs");
    }
    PathBuf::from(LOG_DIR)
}

/// Find a command on PATH.
fn which_command(name: &str) -> Option<String> {
    Command::new("which").arg(name).output().ok().and_then(|o| {
        if o.status.success() {
            String::from_utf8(o.stdout)
                .ok()
                .map(|s| s.trim().to_string())
        } else {
            None
        }
    })
}

// ─────────────────────────────────────────────────────────────
//  ReaperProcess — spawn/kill guard
// ─────────────────────────────────────────────────────────────

/// RAII guard that spawns a REAPER process and kills it on drop.
pub struct ReaperProcess {
    child: Child,
    socket_path: PathBuf,
}

impl ReaperProcess {
    /// Compute the PID-based socket path for this process.
    fn socket_path_for_pid(pid: u32) -> PathBuf {
        PathBuf::from(format!("{}{}{}", SOCKET_PREFIX, pid, SOCKET_SUFFIX))
    }

    /// Get the socket path for this REAPER instance.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Get the PID of this REAPER instance.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Spawn REAPER with no project (empty session).
    pub fn spawn() -> Result<Self> {
        Self::spawn_inner(None, &[])
    }

    /// Spawn REAPER with a specific project file.
    pub fn spawn_with_project(project_path: &str) -> Result<Self> {
        Self::spawn_inner(None, &[project_path.to_string()])
    }

    /// Spawn REAPER with environment variables and optional extra args.
    pub fn spawn_with_env(env: &[(&str, &str)], extra_args: &[&str]) -> Result<Self> {
        Self::spawn_inner(
            Some(env),
            &extra_args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
    }

    /// Spawn REAPER from a full `DawInstanceConfig`.
    pub fn spawn_config(config: &DawInstanceConfig) -> Result<Self> {
        let env_pairs: Vec<(&str, &str)> = config
            .env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let args: Vec<String> = config.args.clone();

        // Check if FTS_SOCKET is explicitly set (deterministic socket path).
        // This avoids relying on PID matching, which breaks when REAPER forks
        // internally (the child PID differs from the spawned PID).
        let explicit_socket = config
            .env
            .iter()
            .find(|(k, _)| k == "FTS_SOCKET")
            .map(|(_, v)| PathBuf::from(v));

        if config.is_rig_wrapper {
            let socket_path = explicit_socket
                .ok_or_else(|| eyre::eyre!("for_rig config missing FTS_SOCKET in env"))?;

            let exe = config
                .executable
                .as_deref()
                .ok_or_else(|| eyre::eyre!("for_rig config missing executable"))?;

            let _ = std::fs::remove_file(&socket_path);

            let mut cmd = std::process::Command::new(exe);
            cmd.args(&args);
            for (k, v) in &env_pairs {
                cmd.env(k, v);
            }
            let child = cmd
                .spawn()
                .map_err(|e| eyre::eyre!("Failed to spawn rig wrapper at {exe}: {e}"))?;

            println!(
                "  Spawned rig wrapper (PID {}), socket: {}",
                child.id(),
                socket_path.display()
            );
            return Ok(Self { child, socket_path });
        }

        if let Some(socket_path) = explicit_socket {
            // Explicit socket path — spawn REAPER and use the given path
            // instead of deriving from PID.
            let exe = config.executable.clone().unwrap_or_else(reaper_executable);

            let _ = std::fs::remove_file(&socket_path);

            let mut cmd = Command::new(&exe);
            cmd.arg("-newinst").arg("-nosplash").arg("-ignoreerrors");
            for arg in &args {
                cmd.arg(arg);
            }
            for (k, v) in &env_pairs {
                cmd.env(k, v);
            }

            println!("  CMD: {exe} {:?}", cmd.get_args().collect::<Vec<_>>());

            let child = cmd
                .spawn()
                .map_err(|e| eyre::eyre!("Failed to spawn REAPER at {exe}: {e}"))?;

            println!(
                "  Spawned REAPER (PID {}), socket: {}",
                child.id(),
                socket_path.display()
            );
            return Ok(Self { child, socket_path });
        }

        Self::spawn_inner_with_exe(
            if env_pairs.is_empty() {
                None
            } else {
                Some(&env_pairs)
            },
            &args,
            config.executable.as_deref(),
            config.resources.as_deref(),
        )
    }

    fn spawn_inner(env: Option<&[(&str, &str)]>, extra_args: &[String]) -> Result<Self> {
        Self::spawn_inner_with_exe(env, extra_args, None, None)
    }

    fn spawn_inner_with_exe(
        env: Option<&[(&str, &str)]>,
        extra_args: &[String],
        custom_exe: Option<&str>,
        custom_resources: Option<&str>,
    ) -> Result<Self> {
        let exe = custom_exe
            .map(String::from)
            .unwrap_or_else(reaper_executable);
        let resources = custom_resources
            .map(String::from)
            .unwrap_or_else(reaper_resources);
        let mut cmd = Command::new(&exe);
        cmd.current_dir(&resources)
            .arg("-newinst")
            .arg("-nosplash")
            .arg("-ignoreerrors");

        for arg in extra_args {
            cmd.arg(arg);
        }

        if let Some(env_pairs) = env {
            for (key, value) in env_pairs {
                cmd.env(key, value);
            }
        }

        // Debug: show actual command being run
        println!("  CMD: {exe} {:?}", cmd.get_args().collect::<Vec<_>>());

        let child = cmd
            .spawn()
            .map_err(|e| eyre::eyre!("Failed to spawn REAPER at {exe}: {e}"))?;

        let pid = child.id();
        let socket_path = Self::socket_path_for_pid(pid);

        // Remove stale socket from a previous run with the same PID
        let _ = std::fs::remove_file(&socket_path);

        println!(
            "  Spawned REAPER (PID {pid}), socket: {}",
            socket_path.display()
        );
        Ok(Self { child, socket_path })
    }

    /// Block until the Unix socket appears (REAPER is ready for connections).
    pub fn wait_for_socket(&self) -> Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(REAPER_BOOT_TIMEOUT_SECS);
        print!("  Waiting for socket {}", self.socket_path.display());
        while !self.socket_path.exists() {
            if std::time::Instant::now() > deadline {
                println!();
                return Err(eyre::eyre!(
                    "Timed out after {REAPER_BOOT_TIMEOUT_SECS}s waiting for {}",
                    self.socket_path.display()
                ));
            }
            std::thread::sleep(Duration::from_millis(500));
            print!(".");
        }
        println!("\n  Socket ready");
        Ok(())
    }
}

impl Drop for ReaperProcess {
    fn drop(&mut self) {
        println!("\n  Killing REAPER (PID {})...", self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

// ─────────────────────────────────────────────────────────────
//  DawBackend — the DAW-agnostic seam
// ─────────────────────────────────────────────────────────────

/// Abstraction over the DAW-coupled bits of the harness: locating the
/// executable / resources dir, launching an instance from a
/// [`DawInstanceConfig`], installing a bridge cdylib, discovering the
/// control socket, and tearing the instance down.
///
/// The harness orchestration (`run_daw_test` / `run_multi_daw_test`) goes
/// through this trait so the same tests can later target other DAWs. Today
/// the only concrete impl is [`ReaperBackend`]; do not add others until a
/// non-REAPER target exists.
///
/// The launched handle owns the process lifecycle — dropping it tears the
/// instance down (kills the process, removes the socket).
pub trait DawBackend {
    /// A spawned, not-yet-connected DAW instance. Dropping it tears down.
    type Process: DawProcess;

    /// Path to the DAW executable.
    fn executable(&self) -> String;

    /// Path to the DAW resources / config directory.
    fn resources(&self) -> String;

    /// Launch an instance described by `config`.
    fn launch(&self, config: &DawInstanceConfig) -> Result<Self::Process>;

    /// Install a built bridge cdylib so the launched DAW exposes the
    /// control socket. `lib_path` is the built library; `lib_name` is the
    /// filename to install under the DAW's plugin directory.
    fn install_bridge(&self, lib_path: &Path, lib_name: &str) -> Result<()>;

    /// Discover an already-running control socket (used when connecting to
    /// an externally-launched DAW), or `None` to fall back to env/PID rules.
    fn discover_socket(&self) -> Option<PathBuf>;
}

/// A spawned DAW instance handle used by [`DawBackend`].
pub trait DawProcess {
    /// Control socket path for this instance.
    fn socket_path(&self) -> &Path;
    /// Process id.
    fn pid(&self) -> u32;
    /// Block until the control socket appears.
    fn wait_for_socket(&self) -> Result<()>;
}

impl DawProcess for ReaperProcess {
    fn socket_path(&self) -> &Path {
        ReaperProcess::socket_path(self)
    }
    fn pid(&self) -> u32 {
        ReaperProcess::pid(self)
    }
    fn wait_for_socket(&self) -> Result<()> {
        ReaperProcess::wait_for_socket(self)
    }
}

/// The single concrete [`DawBackend`] — wraps the existing `ReaperProcess`
/// spawn/teardown logic and the runner's plugin-install helper verbatim.
#[derive(Default)]
pub struct ReaperBackend;

impl DawBackend for ReaperBackend {
    type Process = ReaperProcess;

    fn executable(&self) -> String {
        reaper_executable()
    }

    fn resources(&self) -> String {
        reaper_resources()
    }

    fn launch(&self, config: &DawInstanceConfig) -> Result<Self::Process> {
        ReaperProcess::spawn_config(config)
    }

    fn install_bridge(&self, lib_path: &Path, lib_name: &str) -> Result<()> {
        let resources = PathBuf::from(self.resources());
        let plugins_dir = resources.join("UserPlugins");
        runner::install_plugin(lib_path, lib_name, &plugins_dir)
            .map_err(|e| eyre::eyre!("install_bridge failed: {e}"))
    }

    fn discover_socket(&self) -> Option<PathBuf> {
        runner::find_fts_daw_socket().map(PathBuf::from)
    }
}

// ─────────────────────────────────────────────────────────────
//  Shared DAW connection (singleton per test process)
// ─────────────────────────────────────────────────────────────

/// Shared runtime + DAW connection that persists across all tests.
///
/// Each `#[reaper_test]` creates its own `#[tokio::test]` runtime, but the
/// VOX driver task must live on a single runtime for the entire process.
/// If the creating runtime drops, the driver dies and all subsequent tests
/// get `DriverGone`. Solving this by owning a long-lived runtime here.
///
/// Uses `ManuallyDrop<Runtime>` so the custom `Drop` impl can consume the
/// runtime and call `shutdown_background()` (non-blocking). This prevents
/// the test binary from hanging indefinitely when REAPER is killed without
/// a clean disconnect — the VOX driver task would otherwise block `Runtime::drop`.
struct SharedState {
    runtime: ManuallyDrop<Runtime>,
    daw: Daw,
}

impl Drop for SharedState {
    fn drop(&mut self) {
        // SAFETY: `runtime` is only taken here in `drop`, never accessed afterwards.
        let runtime = unsafe { ManuallyDrop::take(&mut self.runtime) };
        // Non-blocking shutdown: abandon any still-running tasks (e.g. the VOX
        // driver task stuck waiting on a killed REAPER socket) without blocking.
        runtime.shutdown_background();
    }
}

static SHARED: OnceLock<SharedState> = OnceLock::new();
/// Serializes initialization so only one thread builds the runtime + connection.
static INIT_LOCK: Mutex<()> = Mutex::new(());

/// Get or create the shared DAW connection (and its runtime).
///
/// Established once and reused across all parallel tests.
/// Only project tab creation is per-test.
fn shared_daw() -> Result<Daw> {
    if let Some(state) = SHARED.get() {
        return Ok(state.daw.clone());
    }

    // Serialize initialization across threads
    let _guard = INIT_LOCK.lock().unwrap();

    // Double-check after acquiring lock
    if let Some(state) = SHARED.get() {
        return Ok(state.daw.clone());
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| eyre::eyre!("Failed to build shared runtime: {e}"))?;

    let daw = runtime.block_on(connect_daw())?;
    let _ = SHARED.set(SharedState {
        runtime: ManuallyDrop::new(runtime),
        daw,
    });
    Ok(SHARED.get().unwrap().daw.clone())
}

/// Run a future on the shared runtime (where the VOX driver lives).
///
/// This is the key to making parallel tests work: all DAW calls go through
/// the same runtime that owns the driver task, so it never becomes `DriverGone`.
pub fn block_on_shared<F: Future<Output = T>, T>(f: F) -> T {
    let state = SHARED
        .get()
        .expect("shared_daw() must be called before block_on_shared()");
    state.runtime.block_on(f)
}

// ─────────────────────────────────────────────────────────────
//  Batch project tab pool
// ─────────────────────────────────────────────────────────────

/// How many non-isolated tests share a single project tab before
/// a new tab is created for the next batch.
const BATCH_SIZE: u32 = 10;

/// A project tab shared by a batch of tests.
///
/// The first test in the batch creates the tab. Each test increments
/// `active_count` on entry and decrements on exit. When `active_count`
/// drops to zero AND `slots_claimed >= BATCH_SIZE` (batch is full and
/// all tests done), the tab is closed.
struct BatchTab {
    project: Project,
    guid: String,
    /// How many tests are currently running in this batch.
    active_count: AtomicU32,
    /// How many slots have been claimed (monotonically increasing).
    slots_claimed: AtomicU32,
    /// Signaled when `active_count` reaches 0.
    done: Condvar,
    done_mutex: Mutex<()>,
}

/// Global batch pool.
///
/// `current_batch` holds the batch that new non-isolated tests join.
/// `retired_batches` holds batches that are full but still have active tests.
struct BatchPool {
    current_batch: Option<Arc<BatchTab>>,
    retired_batches: Vec<Arc<BatchTab>>,
}

static BATCH_POOL: Mutex<Option<BatchPool>> = Mutex::new(None);

/// Claim a project tab from the batch pool. Returns the batch and the project.
///
/// If the current batch has room, the test joins it. Otherwise, a new batch
/// is created. The caller must call `release_batch` when the test completes.
async fn claim_batch_tab(daw: &Daw) -> Result<Arc<BatchTab>> {
    {
        let mut pool = BATCH_POOL.lock().unwrap();
        let pool = pool.get_or_insert_with(|| BatchPool {
            current_batch: None,
            retired_batches: Vec::new(),
        });

        // Check if current batch has room.
        if let Some(ref batch) = pool.current_batch {
            let claimed = batch.slots_claimed.fetch_add(1, Ordering::SeqCst) + 1;
            if claimed <= BATCH_SIZE {
                batch.active_count.fetch_add(1, Ordering::SeqCst);
                return Ok(Arc::clone(batch));
            }
            // Batch is full — retire it and fall through to create a new one.
            batch.slots_claimed.fetch_sub(1, Ordering::SeqCst);
            let retired = pool.current_batch.take().unwrap();
            pool.retired_batches.push(retired);
        }
    }

    // Create a new batch tab without holding the global pool mutex.
    let project = daw
        .create_project()
        .await
        .map_err(|e| eyre::eyre!("Failed to create batch project tab: {e}"))?;
    let guid = project.guid().to_string();

    // Clean slate
    project.tracks().remove_all().await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let batch = Arc::new(BatchTab {
        project,
        guid,
        active_count: AtomicU32::new(1),
        slots_claimed: AtomicU32::new(1),
        done: Condvar::new(),
        done_mutex: Mutex::new(()),
    });

    let mut pool = BATCH_POOL.lock().unwrap();
    let pool = pool.get_or_insert_with(|| BatchPool {
        current_batch: None,
        retired_batches: Vec::new(),
    });
    pool.current_batch = Some(Arc::clone(&batch));
    Ok(batch)
}

/// Release a batch tab after a test completes.
///
/// Decrements `active_count`. If this is the last test AND the batch is
/// full (or retired), closes the project tab.
async fn release_batch_tab(daw: &Daw, batch: &Arc<BatchTab>) {
    let prev = batch.active_count.fetch_sub(1, Ordering::SeqCst);
    if prev == 1 {
        // We were the last active test — notify waiters and maybe close
        let _guard = batch.done_mutex.lock().unwrap();
        batch.done.notify_all();
        drop(_guard);

        // Check if this batch is retired (no more tests will join)
        let is_retired = {
            let pool = BATCH_POOL.lock().unwrap();
            if let Some(ref pool) = *pool {
                // Retired if it's not the current batch, OR current batch is full
                let is_current = pool
                    .current_batch
                    .as_ref()
                    .map(|b| Arc::ptr_eq(b, batch))
                    .unwrap_or(false);
                !is_current || batch.slots_claimed.load(Ordering::SeqCst) >= BATCH_SIZE
            } else {
                true
            }
        };

        if is_retired {
            // Close the tab: remove tracks first to avoid save dialog
            let _ = batch.project.tracks().remove_all().await;
            if let Err(e) = daw.close_project(&batch.guid).await {
                eprintln!(
                    "Warning: failed to close batch project tab {}: {e}",
                    &batch.guid[..16.min(batch.guid.len())]
                );
            }

            // Remove from retired list
            let mut pool = BATCH_POOL.lock().unwrap();
            if let Some(ref mut pool) = *pool {
                pool.retired_batches.retain(|b| !Arc::ptr_eq(b, batch));
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Connection
// ─────────────────────────────────────────────────────────────

// Connection uses vox v7 initiator API (no more Connector trait)

/// Connect to a running REAPER instance via vox over Unix socket.
///
/// Uses `FTS_SOCKET` env var if set, otherwise discovers the first
/// `/tmp/fts-daw-*.sock` socket. Polls until the socket exists and
/// a connection is established, then returns a `Daw` handle.
pub async fn connect_daw() -> Result<Daw> {
    connect_daw_at(None).await
}

/// Connect to a specific REAPER socket path, or discover one if `None`.
pub async fn connect_daw_at(socket_override: Option<&Path>) -> Result<Daw> {
    let socket_path = if let Some(path) = socket_override {
        path.to_path_buf()
    } else if let Ok(env_path) = std::env::var("FTS_SOCKET") {
        PathBuf::from(env_path)
    } else {
        // Discover first available PID-based socket
        discover_socket().await?
    };

    // Poll until socket file exists
    let deadline = std::time::Instant::now() + Duration::from_secs(REAPER_BOOT_TIMEOUT_SECS);
    while !socket_path.exists() {
        if std::time::Instant::now() > deadline {
            return Err(eyre::eyre!(
                "Timed out waiting for REAPER socket at {}",
                socket_path.display()
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Brief pause to let the listener fully bind
    tokio::time::sleep(Duration::from_millis(200)).await;

    let stream = tokio::net::UnixStream::connect(&socket_path).await?;
    let link = vox_stream::StreamLink::unix(stream);

    // vox 0.10 lane model: establish the connection, then open the DAW
    // service lane (carries `vox-service: reaper-test` automatically),
    // yielding a ready-to-use `Caller`. The connection handle is kept
    // alive by the caller it produces.
    let connection = vox::initiator_on(link)
        .establish_connection()
        .await
        .map_err(|e| eyre::eyre!("Failed to establish vox connection: {:?}", e))?;
    let client = connection
        .open_lane::<TestLaneClient>()
        .await
        .map_err(|e| eyre::eyre!("Failed to open DAW lane: {:?}", e))?;
    // The lane's `Caller` and its spawned driver keep the session alive;
    // the connection run loop is spawned internally by establish_connection,
    // so dropping the handle here does not tear it down (mirrors the prior
    // detached-driver behaviour, where `session` was dropped at fn end).
    drop(connection);

    Ok(Daw::new(client.caller))
}

/// Minimal `FromVoxLane` client that captures the DAW service lane's
/// `Caller` (vox 0.10 replacement for the removed `NoopClient`).
#[derive(Clone)]
struct TestLaneClient {
    caller: vox::Caller,
}

impl vox::FromVoxLane for TestLaneClient {
    const SERVICE_NAME: &'static str = "reaper-test";

    fn from_vox_lane(caller: vox::Caller, _connection: Option<vox::ConnectionHandle>) -> Self {
        Self { caller }
    }
}

/// Discover the first available `/tmp/fts-daw-*.sock` socket that is actually
/// connectable (filters out stale sockets from crashed sessions).
async fn discover_socket() -> Result<PathBuf> {
    let deadline = std::time::Instant::now() + Duration::from_secs(REAPER_BOOT_TIMEOUT_SECS);
    loop {
        if let Ok(entries) = std::fs::read_dir("/tmp") {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && name.starts_with("fts-daw-")
                    && name.ends_with(".sock")
                    && !name.contains(".bootstrap.")
                {
                    // Verify the socket is connectable (not stale)
                    if tokio::net::UnixStream::connect(&path).await.is_ok() {
                        return Ok(path);
                    } else {
                        // Stale socket — clean it up
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
        if std::time::Instant::now() > deadline {
            return Err(eyre::eyre!(
                "Timed out discovering REAPER socket (no connectable fts-daw-*.sock found in /tmp)"
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

// ─────────────────────────────────────────────────────────────
//  ReaperTestContext
// ─────────────────────────────────────────────────────────────

/// Context passed to each `#[reaper_test]` function.
///
/// For isolated tests, this wraps a dedicated REAPER project tab.
/// For batched tests, this wraps a project tab shared with up to
/// `BATCH_SIZE` other tests — cleanup happens when the whole batch
/// finishes, not per-test.
pub struct DawTestContext {
    /// Connected DAW handle (shared across all tests).
    pub daw: Daw,
    /// This test's project (isolated tab or batch-shared tab).
    pub project: Project,
    /// Path to the test assets directory (where `.RTrackTemplate` files live).
    asset_dir: PathBuf,
    /// Test function name.
    test_name: String,
    /// Per-test log file.
    log_file: Mutex<File>,
}

impl DawTestContext {
    /// Find a track by exact name in this test's project, returning an error if not found.
    pub async fn track_by_name(&self, name: &str) -> Result<TrackHandle> {
        self.project
            .tracks()
            .by_name(name)
            .await?
            .ok_or_else(|| eyre::eyre!("track not found: '{name}'"))
    }

    /// Get this test's isolated project handle.
    ///
    /// This returns the test's own project tab, NOT whatever happens to be
    /// the "current" project in REAPER. Safe for parallel use.
    pub fn project(&self) -> &Project {
        &self.project
    }

    // ── UI assertion helpers ────────────────────────────────────────────
    //
    // Backed by `DockHostService`. Require the host extension running
    // inside REAPER to have registered a `DockHostingDispatcher` —
    // see `daw_reaper_dioxus::register_dock_host_service` (TODO: wire
    // into create_daw_handler under daw-reaper feature).

    /// Look up (or register) a dock by string id and return the handle.
    /// Idempotent: repeat calls for the same id return the same handle.
    pub async fn dock_handle(
        &self,
        id: &str,
        title: &str,
        kind: crate::service::dock_host::DockKind,
    ) -> eyre::Result<crate::service::dock_host::DockHandle> {
        Ok(self.daw.dock_host().register_dock(id, title, kind).await?)
    }

    /// Assert that the panel registered under `id` is currently visible.
    /// Fails with a descriptive error if the dock isn't registered or is
    /// hidden.
    pub async fn assert_panel_visible(&self, id: &str) -> eyre::Result<()> {
        let handle = self
            .daw
            .dock_host()
            .register_dock(id, id, crate::service::dock_host::DockKind::Tabbed)
            .await?;
        let visible = self.daw.dock_host().is_visible(handle).await?;
        if !visible {
            eyre::bail!("expected dock '{id}' to be visible, but it was hidden");
        }
        Ok(())
    }

    /// Whether the panel registered under `id` is currently visible.
    ///
    /// For tests that need to normalise panel state before acting —
    /// panel visibility is global to the REAPER instance, so a test
    /// that follows another inherits whatever the last one left open.
    pub async fn is_panel_visible(&self, id: &str) -> eyre::Result<bool> {
        let handle = self
            .daw
            .dock_host()
            .register_dock(id, id, crate::service::dock_host::DockKind::Tabbed)
            .await?;
        Ok(self.daw.dock_host().is_visible(handle).await?)
    }

    /// Assert that the panel registered under `id` is currently hidden.
    pub async fn assert_panel_hidden(&self, id: &str) -> eyre::Result<()> {
        let handle = self
            .daw
            .dock_host()
            .register_dock(id, id, crate::service::dock_host::DockKind::Tabbed)
            .await?;
        let visible = self.daw.dock_host().is_visible(handle).await?;
        if visible {
            eyre::bail!("expected dock '{id}' to be hidden, but it was visible");
        }
        Ok(())
    }

    /// Get the full path to an asset file in the test assets directory.
    pub fn asset_path(&self, filename: &str) -> PathBuf {
        self.asset_dir.join(filename)
    }

    /// Load a `.RTrackTemplate` file into this test's project.
    ///
    /// Parses `<TRACK>` blocks from the template and inserts each as a new
    /// track with its full chunk state. The template name is resolved relative
    /// to the test assets directory (e.g., `"testing-stockjs-guitar-rig"`
    /// loads `tests/reaper-assets/testing-stockjs-guitar-rig.RTrackTemplate`).
    pub async fn load_template(&self, template_name: &str) -> Result<()> {
        let template_path = self
            .asset_dir
            .join(format!("{template_name}.RTrackTemplate"));
        load_template(&self.project, &template_path).await
    }

    /// Write a message to this test's log file.
    pub fn log(&self, msg: &str) {
        if let Ok(mut f) = self.log_file.lock() {
            let _ = writeln!(f, "{msg}");
        }
    }

    /// Get the path to this test's log file.
    pub fn log_path(&self) -> PathBuf {
        log_dir().join(format!("{}.log", self.test_name))
    }
}

// ─────────────────────────────────────────────────────────────
//  Template loading
// ─────────────────────────────────────────────────────────────

/// Parse `<TRACK ...>` blocks from an RPP / `.RTrackTemplate` file.
///
/// Returns each top-level `<TRACK ... >` block as a separate string,
/// including the `<TRACK` opener and closing `>`.
fn parse_track_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current_block = String::new();
    let mut depth: i32 = 0;
    let mut in_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if !in_block {
            if trimmed.starts_with("<TRACK") {
                in_block = true;
                depth = 1;
                current_block.clear();
                current_block.push_str(line);
                current_block.push('\n');
            }
        } else {
            current_block.push_str(line);
            current_block.push('\n');

            if trimmed.starts_with('<') {
                depth += 1;
            } else if trimmed == ">" {
                depth -= 1;
                if depth == 0 {
                    blocks.push(current_block.clone());
                    current_block.clear();
                    in_block = false;
                }
            }
        }
    }

    blocks
}

/// Load a `.RTrackTemplate` file into a specific project tab.
///
/// For each `<TRACK>` block in the template:
/// 1. Insert a new track via `add_track`
/// 2. Set its full state via `set_track_chunk`
async fn load_template(project: &Project, template_path: &Path) -> Result<()> {
    let content = std::fs::read_to_string(template_path)
        .map_err(|e| eyre::eyre!("Failed to read template {}: {e}", template_path.display()))?;

    let blocks = parse_track_blocks(&content);
    if blocks.is_empty() {
        return Err(eyre::eyre!(
            "No <TRACK> blocks found in {}",
            template_path.display()
        ));
    }

    println!(
        "  Loading template: {} ({} tracks)",
        template_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy(),
        blocks.len()
    );

    // `TrackHandle::set_chunk` retired alongside the architect::rpc
    // port — chunk-based bulk import lives on a sibling trait when
    // revived. Until then this template helper is a no-op shell.
    let _tracks = project;
    let _ = blocks;

    // Brief settle time for REAPER to process the chunks.
    tokio::time::sleep(Duration::from_millis(500)).await;

    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Test runner
// ─────────────────────────────────────────────────────────────

/// The function type that `#[reaper_test]` generates for test bodies.
pub type TestBodyFn = dyn Fn(&DawTestContext) -> Pin<Box<dyn Future<Output = Result<()>> + '_>>;

/// Run a single REAPER integration test.
///
/// This is called by the code generated by `#[reaper_test]`. It:
/// 1. Gets the shared DAW connection (established once per process)
/// 2. If `isolated`, creates a dedicated project tab for this test alone.
///    Otherwise, claims a slot in a **batch** of up to `BATCH_SIZE` tests
///    that share one project tab. The tab is closed only when all tests
///    in the batch finish — no per-test cleanup races.
/// 3. Constructs a [`ReaperTestContext`] and calls the test body
/// 4. Cleans up: isolated tests close their tab immediately; batched tests
///    decrement their batch counter and the last one out closes the tab.
///
/// This function is **synchronous** — it runs all async work on the shared
/// runtime via `block_on_shared`. This prevents `DriverGone` errors from
/// each `#[test]` getting a separate tokio runtime.
pub fn run_daw_test(
    test_name: &str,
    isolated: bool,
    body: impl Fn(&DawTestContext) -> Pin<Box<dyn Future<Output = Result<()>> + '_>>,
) -> Result<()> {
    // Ensure log directory exists
    let log_dir = log_dir();
    fs::create_dir_all(&log_dir).map_err(|e| eyre::eyre!("Failed to create log dir: {e}"))?;

    let log_path = log_dir.join(format!("{test_name}.log"));
    let log_file = File::create(&log_path)
        .map_err(|e| eyre::eyre!("Failed to create log file {}: {e}", log_path.display()))?;

    // Get the shared DAW connection (also initializes the shared runtime)
    let daw = shared_daw()?;

    // All async work runs on the shared runtime
    block_on_shared(async {
        // Acquire a project tab — either isolated or from the batch pool
        let batch: Option<Arc<BatchTab>>;
        let project: Project;
        let is_own_tab: bool;

        if isolated {
            // Create a dedicated project tab for this test
            let p = daw
                .create_project()
                .await
                .map_err(|e| eyre::eyre!("[{test_name}] Failed to create project tab: {e}"))?;

            println!(
                "[{test_name}] Created isolated project tab (guid: {})",
                p.guid()
            );

            // Remove default tracks from new project tab
            p.tracks().remove_all().await?;
            tokio::time::sleep(Duration::from_millis(200)).await;

            project = p;
            batch = None;
            is_own_tab = true;
        } else {
            // Claim a slot in the current batch
            let b = claim_batch_tab(&daw).await?;
            project = b.project.clone();
            batch = Some(b);
            is_own_tab = false;
        };

        // Determine asset directory from CARGO_MANIFEST_DIR of the test crate.
        let asset_dir = std::env::var("CARGO_MANIFEST_DIR")
            .map(|d| PathBuf::from(d).join("tests").join("reaper-assets"))
            .unwrap_or_else(|_| PathBuf::from("tests/reaper-assets"));

        let ctx = DawTestContext {
            daw: daw.clone(),
            project: project.clone(),
            asset_dir,
            test_name: test_name.to_string(),
            log_file: Mutex::new(log_file),
        };

        // Run the test body
        let result = body(&ctx).await;

        // Cleanup: always close per-test tabs. `FTS_KEEP_OPEN` only
        // controls whether the REAPER process itself stays alive after
        // the suite finishes (see runner.rs::RunningReaper::stop) —
        // leaking N tabs for N tests pollutes the inspection state and
        // wedges the active-tab logic. Inspection happens on a clean
        // REAPER after all tests have run.
        if is_own_tab {
            // Isolated: remove all tracks then close the tab
            let project_guid = project.guid().to_string();
            let _ = project.tracks().remove_all().await;
            if let Err(e) = daw.close_project(&project_guid).await {
                eprintln!("[{test_name}] Warning: failed to close project tab: {e}");
            }
        } else if let Some(ref b) = batch {
            // Batched: release our slot — last test out closes the tab
            release_batch_tab(&daw, b).await;
        }

        // On failure, print the log file path
        if result.is_err() {
            eprintln!("[{test_name}] FAILED — log: {}", log_path.display());
        }

        result
    })
}

// ─────────────────────────────────────────────────────────────
//  Final cleanup
// ─────────────────────────────────────────────────────────────

/// Remove all tracks and close all project tabs except the initial one.
///
/// Call this after all tests complete to leave REAPER in a clean state.
/// Useful when running a subset of tests against an already-open REAPER
/// instance where the batch system doesn't reach `BATCH_SIZE` and
/// therefore never triggers its own cleanup.
pub async fn cleanup_all_projects(daw: &Daw) -> Result<()> {
    let projects = daw.projects().await?;
    println!("  Cleaning up {} project tab(s)...", projects.len());

    for (i, project) in projects.iter().enumerate() {
        let track_count = project.tracks().count().await.unwrap_or(0);
        if track_count > 0 {
            println!("    Tab {}: removing {} tracks", i, track_count);
            let _ = project.tracks().remove_all().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Close all tabs except the first (REAPER's initial tab)
        if i > 0 {
            println!(
                "    Closing tab {} (guid: {})",
                i,
                &project.guid()[..8.min(project.guid().len())]
            );
            let _ = daw.close_project(&project.guid().to_string()).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    // Drain the batch pool so it doesn't hold stale references
    let mut pool = BATCH_POOL.lock().unwrap();
    *pool = None;

    println!("  Cleanup complete");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Multi-instance test support
// ─────────────────────────────────────────────────────────────

/// Description of a REAPER instance to spawn for multi-DAW tests.
pub struct DawInstanceConfig {
    /// Human-readable label (e.g., "primary", "secondary").
    pub label: String,
    /// Environment variables to set.
    pub env: Vec<(String, String)>,
    /// Extra CLI arguments for REAPER.
    pub args: Vec<String>,
    /// Custom executable path (None = default FTS-TESTING).
    pub executable: Option<String>,
    /// Custom resources path (None = derived from executable).
    pub resources: Option<String>,
    /// When true, the executable is a rig wrapper (already contains -newinst etc.
    /// via launch.json). The socket path is read from FTS_SOCKET in `env` rather
    /// than being derived from the spawned PID.
    pub is_rig_wrapper: bool,
}

impl DawInstanceConfig {
    /// Create a config with just a label (no env vars, no extra args).
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            env: Vec::new(),
            args: Vec::new(),
            executable: None,
            resources: None,
            is_rig_wrapper: false,
        }
    }

    /// Create a config targeting a rig installed by `cargo xtask setup-rigs`.
    ///
    /// - Executable: `~/.local/bin/{rig_id}` (the wrapper script)
    /// - Socket: `/tmp/fts-daw-{rig_id}.sock` via `FTS_SOCKET` (avoids PID mismatch
    ///   from the bwrap/reaper-env container forking a child with a different PID)
    /// - REAPER launch args are provided by the rig's `launch.json` — not added here.
    pub fn for_rig(label: &str, rig_id: &str) -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Self {
            label: label.to_string(),
            env: vec![(
                "FTS_SOCKET".to_string(),
                format!("/tmp/fts-daw-{rig_id}.sock"),
            )],
            args: Vec::new(),
            executable: Some(format!("{home}/.local/bin/{rig_id}")),
            resources: None,
            is_rig_wrapper: true,
        }
    }

    /// Create a config for a specific FTS app bundle (e.g., "FTS-VOCALS", "FTS-GUITAR").
    ///
    /// On macOS, resolves `.app` bundle paths from the FTS home directory.
    /// On Linux, converts the app name to a rig ID (e.g., "FTS-TESTING" → "fts-daw-test")
    /// and delegates to [`Self::for_rig`].
    pub fn for_app(label: &str, app_name: &str) -> Self {
        if cfg!(target_os = "linux") {
            // Convert app name to rig ID convention: "FTS-TESTING" → "fts-daw-test"
            let rig_id = format!("fts-{}", app_name.trim_start_matches("FTS-").to_lowercase());
            return Self::for_rig(label, &rig_id);
        }

        let fts = fts_home();
        let app_path = format!("{fts}/Reaper/{app_name}.app");
        Self {
            label: label.to_string(),
            env: Vec::new(),
            args: Vec::new(),
            executable: Some(format!("{app_path}/Contents/MacOS/REAPER")),
            resources: Some(format!("{app_path}/Contents/Resources")),
            is_rig_wrapper: false,
        }
    }

    /// Add an environment variable.
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    /// Add a CLI argument.
    pub fn with_arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_string());
        self
    }

    /// Use the FastTrackStudio REAPER config directory.
    ///
    /// Adds `-cfgfile ~/fasttrackstudio/reaper.ini` so the instance
    /// uses the FTS config (which has `audiodriver=2` for headless playback, extensions, etc.)
    pub fn with_fts_config(mut self) -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let ini = std::env::var("FTS_REAPER_CONFIG")
            .map(|p| format!("{}/reaper.ini", p.replace("$HOME", &home)))
            .unwrap_or_else(|_| format!("{home}/fasttrackstudio/reaper.ini"));
        self.args.extend(["-cfgfile".to_string(), ini]);
        self
    }

    /// Set `FTS_SOCKET` to a deterministic path based on the label.
    ///
    /// This avoids PID-based socket paths which can fail when REAPER forks
    /// internally (the child PID differs from the spawned PID).
    pub fn with_socket(mut self, path: &str) -> Self {
        self.env.push(("FTS_SOCKET".to_string(), path.to_string()));
        self
    }
}

/// A connected REAPER instance for multi-DAW tests.
pub struct DawInstance {
    /// Human-readable label from the config.
    pub label: String,
    /// Connected DAW handle.
    pub daw: Daw,
    /// PID of the REAPER process.
    pub pid: u32,
    /// Socket path used for the connection.
    pub socket_path: PathBuf,
}

impl DawInstance {
    /// Poll an ExtState key until it matches `expected`, with retries.
    pub async fn poll_ext_state(
        &self,
        section: &str,
        key: &str,
        expected: &str,
        retries: u32,
        interval_ms: u64,
    ) -> Result<()> {
        let ext = self.daw.ext_state();
        let mut last_val = String::new();
        for i in 0..retries {
            match ext.get(section, key).await {
                Ok(Some(val)) => {
                    if val == expected {
                        return Ok(());
                    }
                    last_val = val;
                }
                Ok(None) => {
                    last_val = "<none>".to_string();
                }
                Err(e) => {
                    last_val = format!("<err: {e}>");
                }
            }
            if i < retries - 1 {
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }
        }
        Err(eyre::eyre!(
            "[{}] ExtState {section}/{key} did not reach '{expected}' after {retries} retries (last value: '{last_val}')",
            self.label,
        ))
    }

    /// Poll an ExtState key until it has any non-empty value.
    pub async fn poll_ext_state_value(
        &self,
        section: &str,
        key: &str,
        retries: u32,
        interval_ms: u64,
    ) -> Result<String> {
        let ext = self.daw.ext_state();
        for i in 0..retries {
            if let Ok(Some(val)) = ext.get(section, key).await
                && !val.is_empty()
            {
                return Ok(val);
            }
            if i < retries - 1 {
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }
        }
        Err(eyre::eyre!(
            "[{}] ExtState {section}/{key} never got a value after {retries} retries",
            self.label,
        ))
    }
}

/// Context for multi-instance REAPER tests.
///
/// Unlike `ReaperTestContext` (which wraps a single shared DAW), this
/// holds multiple independently-spawned REAPER instances, each with
/// its own connection and process lifecycle.
pub struct MultiDawTestContext {
    /// All spawned and connected REAPER instances.
    pub instances: Vec<DawInstance>,
    /// Test name (for logging).
    pub test_name: String,
    /// Keeps REAPER processes alive — dropped (killed) when the context is dropped.
    _processes: Vec<ReaperProcess>,
}

impl MultiDawTestContext {
    /// Get an instance by label.
    pub fn by_label(&self, label: &str) -> &DawInstance {
        self.instances
            .iter()
            .find(|i| i.label == label)
            .unwrap_or_else(|| panic!("no instance with label '{label}'"))
    }

    /// Assert that two instances have positions within `tolerance_secs`.
    ///
    /// Returns `(pos_a, pos_b, drift)` on success.
    pub async fn assert_positions_close(
        &self,
        label_a: &str,
        label_b: &str,
        tolerance_secs: f64,
        context: &str,
    ) -> Result<(f64, f64, f64)> {
        let a = self.by_label(label_a);
        let b = self.by_label(label_b);
        let proj_a = a.daw.current_project().await?;
        let proj_b = b.daw.current_project().await?;
        let state_a = proj_a.transport().get_state().await?;
        let state_b = proj_b.transport().get_state().await?;
        let pos_a = state_a
            .playhead_position
            .time
            .as_ref()
            .map(|t| t.as_seconds())
            .unwrap_or(0.0);
        let pos_b = state_b
            .playhead_position
            .time
            .as_ref()
            .map(|t| t.as_seconds())
            .unwrap_or(0.0);
        let drift = (pos_a - pos_b).abs();
        assert!(
            drift < tolerance_secs,
            "{context}: position drift {drift:.4}s exceeds {tolerance_secs}s \
             ({label_a}={pos_a:.4}s, {label_b}={pos_b:.4}s)"
        );
        Ok((pos_a, pos_b, drift))
    }

    /// Assert that ALL instance pairs have positions within `tolerance_secs`.
    ///
    /// Reads all positions concurrently (parallel RPC) to minimize measurement
    /// jitter, then checks all unique pairs.
    /// Returns a vec of `(label_a, label_b, pos_a, pos_b, drift)` and the max drift.
    pub async fn assert_all_positions_close(
        &self,
        tolerance_secs: f64,
        context: &str,
    ) -> Result<(Vec<(String, String, f64, f64, f64)>, f64)> {
        // Read all positions concurrently to eliminate sequential read jitter.
        // Each read is an independent RPC call, so they can run in parallel.
        let mut handles = Vec::with_capacity(self.instances.len());
        for inst in &self.instances {
            let label = inst.label.clone();
            let daw = inst.daw.clone();
            handles.push(tokio::spawn(async move {
                let proj = daw.current_project().await?;
                let state = proj.transport().get_state().await?;
                let pos = state
                    .playhead_position
                    .time
                    .as_ref()
                    .map(|t| t.as_seconds())
                    .unwrap_or(0.0);
                Ok::<_, eyre::Report>((label, pos))
            }));
        }
        let mut positions = Vec::with_capacity(handles.len());
        for handle in handles {
            positions.push(handle.await.map_err(|e| eyre::eyre!("join error: {e}"))??);
        }

        let mut results = Vec::new();
        let mut max_drift: f64 = 0.0;

        // Check all unique pairs
        for i in 0..positions.len() {
            for j in (i + 1)..positions.len() {
                let drift = (positions[i].1 - positions[j].1).abs();
                max_drift = max_drift.max(drift);
                assert!(
                    drift < tolerance_secs,
                    "{context}: position drift {drift:.4}s exceeds {tolerance_secs}s \
                     ({}={:.4}s, {}={:.4}s)",
                    positions[i].0,
                    positions[i].1,
                    positions[j].0,
                    positions[j].1,
                );
                results.push((
                    positions[i].0.clone(),
                    positions[j].0.clone(),
                    positions[i].1,
                    positions[j].1,
                    drift,
                ));
            }
        }

        Ok((results, max_drift))
    }

    /// Connect all instances via sync extension peer mesh.
    ///
    /// Wait for sync peers to discover each other via mDNS.
    ///
    /// Waits for each instance's sync extension to be ready, then waits for
    /// mDNS-based peer discovery to connect all instances automatically.
    /// No manual connection strings are written — peers find each other
    /// through Avahi/mDNS service advertisement and discovery.
    ///
    /// `ext_section` is typically `"FTS_SYNC_EXT"`.
    pub async fn connect_sync_peers(&self, ext_section: &str) -> Result<()> {
        // Wait for all extensions to report ready
        for inst in &self.instances {
            inst.poll_ext_state(ext_section, "status", "ready", 30, 1000)
                .await?;
            println!("  [{}] sync extension ready", inst.label);
        }

        // Read peer IDs for logging
        for inst in &self.instances {
            let peer_id = inst
                .poll_ext_state_value(ext_section, "peer_id", 10, 500)
                .await?;
            let port = inst
                .poll_ext_state_value(ext_section, "mesh_port", 10, 500)
                .await?;
            println!("  [{}] peer_id={peer_id}, mesh_port={port}", inst.label);
        }

        println!("  Waiting for mDNS peer discovery...");

        // Wait for all instances to see expected peer count via mDNS
        let expected_peers = (self.instances.len() - 1).to_string();
        for inst in &self.instances {
            inst.poll_ext_state(ext_section, "peer_count", &expected_peers, 30, 1000)
                .await?;
            println!(
                "  [{}] connected to {} peer(s) via mDNS",
                inst.label, expected_peers
            );
        }

        Ok(())
    }

    /// Connect sync peers via direct TCP (bypasses mDNS).
    ///
    /// Reads peer IDs and mesh ports from each instance, then writes
    /// a `connect_peers` ExtState entry to trigger direct TCP connections.
    /// Use this when mDNS is disabled (`FTS_SYNC_NO_MDNS=1`).
    ///
    /// `ext_section` is typically `"FTS_SYNC_EXT"`.
    pub async fn connect_sync_peers_direct(&self, ext_section: &str) -> Result<()> {
        // Wait for all extensions to report ready
        for inst in &self.instances {
            inst.poll_ext_state(ext_section, "status", "ready", 30, 1000)
                .await?;
            println!("  [{}] sync extension ready", inst.label);
        }

        // Read mesh ports and peer IDs
        let mut peers = Vec::new();
        for inst in &self.instances {
            let port = inst
                .poll_ext_state_value(ext_section, "mesh_port", 10, 500)
                .await?;
            let peer_id = inst
                .poll_ext_state_value(ext_section, "peer_id", 10, 500)
                .await?;
            println!("  [{}] peer_id={peer_id}, mesh_port={port}", inst.label);
            peers.push((inst.label.clone(), peer_id, port));
        }

        // Build connect string and trigger connections
        let connect_str: String = peers
            .iter()
            .map(|(_, peer_id, port)| format!("{peer_id}@127.0.0.1:{port}"))
            .collect::<Vec<_>>()
            .join(",");

        for inst in &self.instances {
            inst.daw
                .ext_state()
                .set(ext_section, "connect_peers", &connect_str, false)
                .await?;
        }
        println!("  Triggered direct peer connections: {connect_str}");

        // Wait for all instances to see expected peer count
        let expected_peers = (self.instances.len() - 1).to_string();
        for inst in &self.instances {
            inst.poll_ext_state(ext_section, "peer_count", &expected_peers, 30, 1000)
                .await?;
            println!("  [{}] connected to {} peer(s)", inst.label, expected_peers);
        }

        Ok(())
    }
}

/// Run a multi-instance REAPER test.
///
/// Spawns one REAPER process per config, waits for sockets, connects to
/// each, then calls the test body with a `MultiDawTestContext`. All REAPER
/// processes are killed on exit (success or failure).
///
/// # Example
///
/// ```ignore
/// #[test]
/// #[ignore]
/// fn two_daw_test() -> eyre::Result<()> {
///     run_multi_reaper_test(
///         "two_daw_test",
///         vec![
///             DawInstanceConfig::new("session"),
///             DawInstanceConfig::new("signal").with_env("FTS_DAW_ROLE", "signal"),
///         ],
///         |ctx| Box::pin(async move {
///             let session = ctx.by_label("session");
///             let signal = ctx.by_label("signal");
///             // ... test logic ...
///             Ok(())
///         }),
///     )
/// }
/// ```
pub fn run_multi_daw_test(
    test_name: &str,
    configs: Vec<DawInstanceConfig>,
    body: impl Fn(&MultiDawTestContext) -> Pin<Box<dyn Future<Output = Result<()>> + '_>>,
) -> Result<()> {
    run_multi_daw_test_with(&ReaperBackend, test_name, configs, body)
}

/// Backend-generic core of [`run_multi_daw_test`]. Spawns one instance per
/// config through `backend`, waits for sockets, connects to each, then calls
/// the test body with a [`MultiDawTestContext`]. All instances are torn down
/// on exit (success or failure).
pub fn run_multi_daw_test_with<B: DawBackend>(
    backend: &B,
    test_name: &str,
    configs: Vec<DawInstanceConfig>,
    body: impl Fn(&MultiDawTestContext) -> Pin<Box<dyn Future<Output = Result<()>> + '_>>,
) -> Result<()> {
    println!(
        "\n=== {test_name} — spawning {} DAW instance(s) ===",
        configs.len()
    );

    // Spawn instances one at a time, waiting for each socket before starting the next.
    // This ensures the shared reaper.ini patch → launch → restore cycle is sequential,
    // and REAPER's per-instance lock files are created before the next instance starts.
    let mut processes = Vec::with_capacity(configs.len());
    for config in &configs {
        let process = backend
            .launch(config)
            .map_err(|e| eyre::eyre!("[{test_name}] Failed to spawn '{}': {e}", config.label))?;

        println!(
            "  [{}] spawned PID {}, waiting for socket: {}",
            config.label,
            process.pid(),
            process.socket_path().display()
        );

        process
            .wait_for_socket()
            .map_err(|e| eyre::eyre!("[{test_name}] Socket timeout for '{}': {e}", config.label))?;

        println!("  [{}] ready", config.label);
        processes.push(process);
    }

    // Build a runtime and connect to each
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| eyre::eyre!("[{test_name}] Failed to build runtime: {e}"))?;

    let result = rt.block_on(async {
        let mut instances = Vec::with_capacity(configs.len());

        for (i, process) in processes.iter().enumerate() {
            let daw = connect_daw_at(Some(process.socket_path()))
                .await
                .map_err(|e| {
                    eyre::eyre!(
                        "[{test_name}] Failed to connect to '{}': {e}",
                        configs[i].label
                    )
                })?;

            println!("  [{}] Connected (PID {})", configs[i].label, process.pid());

            instances.push(DawInstance {
                label: configs[i].label.clone(),
                daw,
                pid: process.pid(),
                socket_path: process.socket_path().to_path_buf(),
            });
        }

        let ctx = MultiDawTestContext {
            instances,
            test_name: test_name.to_string(),
            _processes: Vec::new(), // processes are kept alive in the outer scope
        };

        body(&ctx).await
    });

    // Processes are dropped here, killing all REAPER instances
    drop(processes);

    match &result {
        Ok(()) => println!("=== {test_name} PASSED ===\n"),
        Err(e) => eprintln!("=== {test_name} FAILED: {e} ===\n"),
    }

    result
}

// ─────────────────────────────────────────────────────────────
//  Source-compat aliases (REAPER-named → DAW-generic canonical)
// ─────────────────────────────────────────────────────────────

/// Alias for [`DawTestContext`] — kept so existing `#[reaper_test]` call
/// sites referencing `ReaperTestContext` keep compiling.
pub type ReaperTestContext = DawTestContext;

/// Alias for [`run_daw_test`].
pub use run_daw_test as run_reaper_test;

/// Alias for [`run_multi_daw_test`].
pub use run_multi_daw_test as run_multi_reaper_test;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_track_block() {
        let input = "<TRACK\n  NAME \"Test\"\n  VOLPAN 1 0\n>\n";
        let blocks = parse_track_blocks(input);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].starts_with("<TRACK"));
        assert!(blocks[0].trim_end().ends_with(">"));
    }

    #[test]
    fn parse_nested_track_block() {
        let input = r#"<TRACK
  NAME "Outer"
  <FXCHAIN
    <VST "ReaEQ" foo.dll
    >
  >
>
"#;
        let blocks = parse_track_blocks(input);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].contains("FXCHAIN"));
    }

    #[test]
    fn parse_multiple_tracks() {
        let input = r#"<TRACK
  NAME "Track 1"
>
<TRACK
  NAME "Track 2"
>
"#;
        let blocks = parse_track_blocks(input);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("Track 1"));
        assert!(blocks[1].contains("Track 2"));
    }
}
