// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! REAPER-backed input runtime used by `reaper-extension`.
//!
//! This wraps the reference `input/*` implementation and keeps a stable API
//! for extension local actions.

// This crate is an FFI bridge to REAPER's C API — raw pointers are pervasive
// and making every function `unsafe` would be impractical. The pointers come
// from REAPER callbacks and are valid for the duration of each call.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod config;
pub mod daw_module;
pub mod infrastructure;
pub mod input;
pub mod keybind_config;
pub mod testing;
pub mod ui;

use config::InputConfig;
use infrastructure::config_watcher::{ConfigWatcher, KeybindWatcher};
use input::handler::InputHandler;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static RUNTIME_BOOTSTRAPPED: AtomicBool = AtomicBool::new(false);
static CURRENT_PROFILE: Lazy<Mutex<Option<InputProfile>>> = Lazy::new(|| Mutex::new(None));
/// Last profile requested via `apply_config`, whether or not it succeeded.
/// Used to retry after styx presets finish loading.
static DESIRED_PROFILE: Lazy<Mutex<Option<InputProfile>>> = Lazy::new(|| Mutex::new(None));
static CONFIG_WATCHER: Lazy<Mutex<Option<ConfigWatcher>>> = Lazy::new(|| Mutex::new(None));
/// Wall-clock of the last keydown that advanced (or started) a which-key
/// sequence. Used by `check_which_key_timeout` to skip expiry while OS
/// keyboard auto-repeat is still pulsing — keyups between auto-repeated
/// keydowns can briefly empty `held_keys`, but the actual user is still
/// holding the key, so a recent keydown means "don't expire yet".
static LAST_INPUT_ACTIVITY: Lazy<Mutex<Option<Instant>>> = Lazy::new(|| Mutex::new(None));
/// Pre-timeout grace period beyond the configured timeout duration.
/// Comfortably wider than the typical auto-repeat interval (~30-60ms).
const TIMEOUT_GRACE: Duration = Duration::from_millis(800);
static KEYBIND_WATCHER: Lazy<Mutex<Option<KeybindWatcher>>> = Lazy::new(|| Mutex::new(None));
static WORKFLOW_WATCHER: Lazy<Mutex<Option<KeybindWatcher>>> = Lazy::new(|| Mutex::new(None));
/// External label (typically the session mode display name) attached to
/// every input event log line via `current_mode_label()`. Set by
/// downstream crates that know about modes; reaper-input itself never
/// writes this. Empty string when unset so log fields stay consistent.
static CURRENT_MODE_LABEL: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

/// Set the label that input event logs (`[keydown]`, `[action]`, etc.)
/// tag onto every line. Pass `mode.display_name()` from session.
pub fn set_mode_label(label: impl Into<String>) {
    if let Ok(mut guard) = CURRENT_MODE_LABEL.lock() {
        *guard = label.into();
    }
}

/// Cheap copy of the current mode label for log macros. Returns an
/// empty string when unset (so the field still serialises consistently).
pub fn current_mode_label() -> String {
    CURRENT_MODE_LABEL
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_default()
}

pub(crate) fn trace_console_msg(message: impl AsRef<str>) {
    let message = message.as_ref().trim_end();
    if !message.is_empty() {
        tracing::info!("{message}");
    }
}

#[derive(Debug, Clone)]
pub struct InputRuntimeConfig {
    pub eat_handled_keys: bool,
    pub context_tags: Vec<String>,
    pub context_vars: Vec<(String, String)>,
}

impl Default for InputRuntimeConfig {
    fn default() -> Self {
        Self {
            eat_handled_keys: true,
            context_tags: Vec::new(),
            context_vars: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum InputProfile {
    FastTrackStudio,
    Logic,
    ProTools,
}

impl InputProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FastTrackStudio => "FastTrackStudio",
            Self::Logic => "Logic",
            Self::ProTools => "Pro Tools",
        }
    }

    fn preset_name(self) -> &'static str {
        match self {
            Self::FastTrackStudio => "fasttrackstudio",
            Self::Logic => "logic",
            Self::ProTools => "reaper",
        }
    }

    fn mouse_profile_name(self) -> &'static str {
        match self {
            Self::FastTrackStudio => "fasttrackstudio",
            Self::Logic => "logic",
            Self::ProTools => "reaper",
        }
    }
}

pub fn register_with_default_keymap(config: InputRuntimeConfig) -> Result<(), String> {
    register_with_config(config)
}

pub fn register_with_config(_config: InputRuntimeConfig) -> Result<(), String> {
    if !INITIALIZED.swap(true, Ordering::Relaxed) {
        input::keybinds::init();
        input::mouse_modifiers::manager::init();
        input::workflows::init();

        input::handler::register_input_handler().map_err(|e| e.to_string())?;
    }

    InputHandler::set_debug_logging(false);
    set_enabled(true);

    Ok(())
}

/// Bootstrap the input runtime for in-process hosts that embed `reaper-input`
/// as a module rather than using the standalone extension binary.
///
/// This mirrors the standalone extension startup sequence: register the input
/// handler, load the initial config, and start the config watchers so changes
/// are hot-reloaded.
pub fn bootstrap_in_process_runtime() {
    if RUNTIME_BOOTSTRAPPED.swap(true, Ordering::Relaxed) {
        return;
    }

    let _ = register_with_config(InputRuntimeConfig::default());

    let config_dir = std::env::var("FTS_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            reaper_high::Reaper::get()
                .resource_path()
                .join("fasttrackstudio/input")
                .into()
        });

    init_config_watcher(config_dir.join("input.styx"));
    load_keybind_presets_from(config_dir.join("keybinds"));
    load_mouse_profiles_from(config_dir.join("keybinds"));
    load_workflows_from(config_dir.join("workflows"));
    load_profile_workflows_from(config_dir.join("keybinds"));
}

pub fn check_and_hook_windows() {
    if !is_enabled() {
        return;
    }

    input::wheel_hook::check_and_hook_arrange_view();
    input::wheel_hook::check_and_hook_midi_editors();
}

pub fn is_enabled() -> bool {
    InputHandler::is_enabled()
}

pub fn set_enabled(enabled: bool) {
    InputHandler::set_enabled(enabled);
}

pub fn toggle_enabled() -> bool {
    InputHandler::toggle()
}

pub fn is_intercepting() -> bool {
    !InputHandler::is_passthrough()
}

pub fn set_intercepting(enabled: bool) {
    InputHandler::set_passthrough(!enabled);
}

pub fn toggle_intercepting() -> bool {
    let enabled = !is_intercepting();
    set_intercepting(enabled);
    enabled
}

pub fn is_debug_logging() -> bool {
    InputHandler::is_debug_logging()
}

pub fn set_debug_logging(enabled: bool) {
    InputHandler::set_debug_logging(enabled);
}

pub fn toggle_debug_logging() -> bool {
    InputHandler::toggle_debug_logging()
}

pub fn current_profile() -> Option<InputProfile> {
    CURRENT_PROFILE.lock().ok().and_then(|g| *g)
}

pub fn set_profile(profile: InputProfile) -> Result<(), String> {
    let preset_ok = input::keybinds::set_preset(profile.preset_name());
    let mouse_ok = input::mouse_modifiers::manager::set_profile(profile.mouse_profile_name());

    if !preset_ok {
        return Err(format!(
            "Failed to activate keybind preset '{}'",
            profile.preset_name()
        ));
    }
    if !mouse_ok {
        return Err(format!(
            "Failed to activate mouse profile '{}'",
            profile.mouse_profile_name()
        ));
    }

    if let Ok(mut state) = CURRENT_PROFILE.lock() {
        *state = Some(profile);
    }

    input::mouse_modifiers::manager::log_state();
    input::which_key_overlay::clear_cache();
    #[cfg(feature = "overlay-prewarm")]
    input::which_key_overlay::prewarm_current_prefixes();
    Ok(())
}

/// Record that input activity (a real keydown) just happened. Drives the
/// wall-clock side of the timeout check so OS keyboard auto-repeat, which
/// can briefly interleave keyup events between repeated keydowns, doesn't
/// trick `check_which_key_timeout` into expiring a held prefix.
pub fn mark_input_activity() {
    if let Ok(mut guard) = LAST_INPUT_ACTIVITY.lock() {
        *guard = Some(Instant::now());
    }
}

/// Check for which-key sequence timeout.
///
/// Called from the REAPER timer callback (~30fps). If a which-key sequence
/// has been idle for longer than `timeout_ms`, it resets and logs a timeout
/// message to the console, and hides the overlay.
///
/// Two guards skip the expire path:
///   1. The anchor key is still in `held_keys` (steady-state hold).
///   2. The wall-clock since the last keydown is shorter than the
///      configured timeout plus a small grace window. Covers the case
///      where OS auto-repeat emits keyup+keydown pairs and `held_keys`
///      transiently empties.
pub fn check_which_key_timeout() {
    if !input::processor::needs_timeout() {
        return;
    }
    if input::processor::is_anchor_held() {
        return;
    }
    if let Ok(guard) = LAST_INPUT_ACTIVITY.lock()
        && let Some(last) = *guard
        && last.elapsed() < TIMEOUT_GRACE
    {
        return;
    }
    let commands = input::processor::timeout_expired();
    if commands.is_empty() {
        // Timeout expired with no match — hide the overlay
        input::which_key_overlay::hide();
    }
}

/// Refresh the which-key overlay position/render.
///
/// Called from the timer callback so the overlay tracks the arrange view
/// if the user resizes or moves the window.
pub fn refresh_which_key_overlay() {
    input::which_key_overlay::refresh();
}

/// Start building the process-wide wgpu Instance/Adapter/Device on a
/// background thread. Safe to call from `plugin_main` — no window or
/// surface is involved, just GPU driver init. Once it completes, every
/// subsequent overlay build skips ~100ms+ of per-overlay adapter+device
/// setup, which is the bulk of cold-overlay cost.
pub fn prewarm_gpu_in_background() {
    reaper_embed::prewarm_shared_gpu();
}

/// Queue all current which-key prefixes for background prewarming.
///
/// Cheap and synchronous — only reads keybind state. Pair with
/// [`prewarm_which_key_overlay_step`] called once per host timer tick to
/// actually allocate the wgpu/Vello surfaces, one overlay per tick, so the
/// host UI thread stays responsive.
pub fn enqueue_which_key_overlay_prewarm() {
    input::which_key_overlay::enqueue_prewarm();
}

/// Build at most one queued overlay this tick. Returns `true` if more work
/// remains for subsequent ticks, `false` once the queue is drained.
///
/// Must run on the host's main thread (the same thread that pumps REAPER's
/// message loop), after the host window is realised.
pub fn prewarm_which_key_overlay_step() -> bool {
    input::which_key_overlay::prewarm_step()
}

/// Load workflow definitions from a directory of `.styx` files.
///
/// `workflows_dir` should point to e.g. `~/.config/REAPER/FTS/workflows/`.
/// Each `*.styx` file is parsed as a [`WorkflowConfig`] and registered as a
/// default workflow implementation in the global workflow manager.
pub fn load_workflows_from(workflows_dir: PathBuf) {
    input::workflows::load_workflows_from_dir(&workflows_dir);
    // Seed the workflow watcher so subsequent edits to `*.styx` files
    // in this dir trigger a hot-reload via `check_config_reload`. The
    // keybinds path does the same inline in `load_keybind_presets_from`;
    // doing it here keeps both watchers symmetrically wired.
    if let Ok(mut guard) = WORKFLOW_WATCHER.lock() {
        *guard = Some(KeybindWatcher::new(workflows_dir.clone()));
    }
    crate::trace_console_msg(format!(
        "FTS Input workflows loaded: dir={}\n",
        workflows_dir.display()
    ));
}

/// Load keybind presets and overlays from a config directory.
///
/// `keybinds_dir` should point to the `keybinds/` subdirectory inside the FTS
/// config dir (e.g. `~/.config/REAPER/FTS/keybinds/`).
///
/// Profile subdirectories and `overlays/*.styx` files are scanned and merged
/// into the running processor alongside the built-in defaults. Safe to call
/// more than once.
pub fn load_keybind_presets_from(keybinds_dir: PathBuf) {
    input::keybinds::load_from_dir(&keybinds_dir);
    // Retry the desired profile now that config-file presets are available.
    // This handles the common startup race where apply_config runs before this.
    let desired = DESIRED_PROFILE.lock().ok().and_then(|g| *g);
    if let Some(profile) = desired
        && current_profile().is_none()
    {
        let _ = set_profile(profile);
    }
    if let Ok(mut guard) = KEYBIND_WATCHER.lock() {
        *guard = Some(KeybindWatcher::new(keybinds_dir));
    }
    input::which_key_overlay::clear_cache();
    #[cfg(feature = "overlay-prewarm")]
    input::which_key_overlay::prewarm_current_prefixes();
    log_state_to_console_with_prefix("FTS Input keybinds loaded");
}

/// Initialize the workflow directory watcher.
pub fn init_workflow_watcher(path: PathBuf) {
    if let Ok(mut guard) = WORKFLOW_WATCHER.lock() {
        *guard = Some(KeybindWatcher::new(path));
    }
}

/// Load profile-specific workflow implementations from a keybinds config directory.
///
/// Scans each profile subdirectory (e.g. `keybinds/fasttrackstudio/workflows/`)
/// for `.styx` files and registers them as profile-specific workflow implementations.
/// Must be called after [`load_workflows_from`] so global definitions exist.
pub fn load_profile_workflows_from(keybinds_dir: PathBuf) {
    input::workflows::load_profile_workflows_from_dir(&keybinds_dir);
    crate::trace_console_msg(format!(
        "FTS Input profile workflows loaded: keybinds_dir={}\n",
        keybinds_dir.display()
    ));
}

/// Load mouse modifier profiles and overlays from a config directory.
///
/// Scans `keybinds_dir` for `mouse-profile.styx` in each profile subdirectory
/// and for `mouse_settings` sections in `overlays/*.styx` files.
pub fn load_mouse_profiles_from(keybinds_dir: PathBuf) {
    input::mouse_modifiers::manager::load_from_dir(&keybinds_dir);
    log_state_to_console_with_prefix("FTS Input mouse profiles loaded");
}

/// Initialise the config file watcher.
///
/// Call this once at plugin startup. `path` should point to the `.styx` config
/// file (e.g. `~/.config/REAPER/FTS/input.styx`). The watcher writes a default
/// file if none exists, then performs an initial load and applies it.
pub fn init_config_watcher(path: PathBuf) {
    let mut watcher = ConfigWatcher::new(path);
    watcher.ensure_default_config();
    if let Some(cfg) = watcher.load() {
        apply_config(cfg);
    }
    if let Ok(mut guard) = CONFIG_WATCHER.lock() {
        *guard = Some(watcher);
    }
}

/// Apply a loaded [`InputConfig`] to the running input system.
pub fn apply_config(cfg: InputConfig) {
    InputHandler::set_enabled(cfg.enabled);
    InputHandler::set_debug_logging(cfg.debug_logging);
    // Always record the desired profile so load_keybind_presets_from can retry
    // if the styx presets haven't been loaded yet when this runs.
    if let Ok(mut guard) = DESIRED_PROFILE.lock() {
        *guard = Some(cfg.profile);
    }
    if let Err(e) = set_profile(cfg.profile) {
        tracing::warn!("Failed to apply profile from config: {e}");
        crate::trace_console_msg(format!(
            "FTS Input config apply failed: profile={} error={}\n",
            cfg.profile.as_str(),
            e
        ));
    } else {
        log_state_to_console_with_prefix("FTS Input config applied");
    }
}

/// Poll for config file changes and apply if changed.
///
/// Cheap to call — does nothing if the file hasn't changed since the last check.
/// Intended to be called from the 30 Hz timer callback.
pub fn check_config_reload() {
    let cfg = if let Ok(mut guard) = CONFIG_WATCHER.lock() {
        guard.as_mut().and_then(|w| w.check_and_reload())
    } else {
        None
    };

    let keybind_changed = if let Ok(mut guard) = KEYBIND_WATCHER.lock() {
        guard.as_mut().map(|w| w.check()).unwrap_or(false)
    } else {
        false
    };

    let workflow_changed = if let Ok(mut guard) = WORKFLOW_WATCHER.lock() {
        guard.as_mut().map(|w| w.check()).unwrap_or(false)
    } else {
        false
    };

    if cfg.is_none() && !keybind_changed && !workflow_changed {
        return;
    }

    crate::trace_console_msg(format!(
        "FTS Input reload detected: config={} keybinds={} workflows={}\n",
        cfg.is_some(),
        keybind_changed,
        workflow_changed
    ));

    let active_workflow = input::workflows::get_active_workflow_id();
    if active_workflow.is_some() {
        input::workflows::deactivate();
    }

    let keybind_dir = if let Ok(guard) = KEYBIND_WATCHER.lock() {
        guard.as_ref().map(|w| w.dir().to_path_buf())
    } else {
        None
    };
    let workflow_dir = if let Ok(guard) = WORKFLOW_WATCHER.lock() {
        guard.as_ref().map(|w| w.dir().to_path_buf())
    } else {
        None
    };

    if let Some(dir) = keybind_dir.as_ref() {
        tracing::info!("Keybind config changed — reloading from {:?}", dir);
        crate::trace_console_msg(format!("FTS Input keybind reload: dir={}\n", dir.display()));
        input::processor::reload_from_dir(dir);
        input::mouse_modifiers::manager::reload_from_dir(dir);
        input::which_key_overlay::clear_cache();
        #[cfg(feature = "overlay-prewarm")]
        input::which_key_overlay::prewarm_current_prefixes();
        log_state_to_console_with_prefix("FTS Input keybind reload complete");
    }

    if let Some(dir) = workflow_dir.as_ref() {
        tracing::info!("Workflow config changed — reloading from {:?}", dir);
        crate::trace_console_msg(format!(
            "FTS Input workflow reload: dir={}\n",
            dir.display()
        ));
        if let Some(keybind_dir) = keybind_dir.as_ref() {
            input::workflows::reload_from_dirs(dir, keybind_dir);
        }
        log_state_to_console_with_prefix("FTS Input workflow reload complete");
    }

    if let Some(cfg) = cfg {
        apply_config(cfg);
    }

    if let Some(id) = active_workflow {
        if let Err(e) = input::workflows::activate(&id) {
            tracing::warn!(workflow = %id, error = %e, "Failed to restore active workflow after reload");
            crate::trace_console_msg(format!(
                "FTS Input workflow restore failed: workflow={} error={}\n",
                id, e
            ));
        } else {
            log_state_to_console_with_prefix("FTS Input workflow restored after reload");
        }
    }
}

pub fn log_state_to_console() {
    log_state_to_console_with_prefix("FTS Input state");
}

fn log_state_to_console_with_prefix(prefix: &str) {
    crate::trace_console_msg(format!(
        "{}: enabled={} intercept={} debug={} profile={} preset={} workflow={:?}\n",
        prefix,
        is_enabled(),
        is_intercepting(),
        is_debug_logging(),
        current_profile().map(|p| p.as_str()).unwrap_or("none"),
        input::keybinds::active_preset_name(),
        input::workflows::active_workflow_name(),
    ));
}
