//! REAPER instance launcher.
//!
//! Reads `launch.json` using a three-tier discovery strategy:
//!
//! 1. `--config <path>` CLI argument (stripped before passing args to REAPER)
//! 2. `FTS_LAUNCH_CONFIG` environment variable
//! 3. `../launch.json` relative to the binary (macOS `.app` bundle layout)
//!
//! After loading the config, patches `reaper.ini`, sets environment variables,
//! and execs REAPER. If `restore_ini_after_launch` is set, forks first so the
//! parent can restore the original INI values after REAPER has started.

use crate::config::LaunchConfig;
use crate::ini::ReaperIni;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

/// Discover the `launch.json` config path using three-tier strategy:
///
/// 1. `--config <path>` CLI argument
/// 2. `FTS_LAUNCH_CONFIG` environment variable
/// 3. `../launch.json` relative to the current executable (macOS `.app` bundle)
///
/// Returns the resolved path and any remaining CLI args (with `--config` stripped).
pub fn discover_config() -> (PathBuf, Option<String>, Vec<String>) {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // 1. Check for --config <path> and --rig <id> in CLI args
    let mut remaining_args = Vec::new();
    let mut config_from_cli = None;
    let mut rig_id = None;
    let mut skip_next = false;
    for (i, arg) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--config"
            && let Some(path) = args.get(i + 1)
        {
            config_from_cli = Some(PathBuf::from(path));
            skip_next = true;
            continue;
        }
        if let Some(path) = arg.strip_prefix("--config=") {
            config_from_cli = Some(PathBuf::from(path));
            continue;
        }
        if arg == "--rig"
            && let Some(id) = args.get(i + 1)
        {
            rig_id = Some(id.clone());
            skip_next = true;
            continue;
        }
        if let Some(id) = arg.strip_prefix("--rig=") {
            rig_id = Some(id.to_string());
            continue;
        }
        remaining_args.push(arg.clone());
    }

    if let Some(path) = config_from_cli {
        return (path, rig_id, remaining_args);
    }

    // 2. Check FTS_LAUNCH_CONFIG environment variable
    if let Ok(path) = std::env::var("FTS_LAUNCH_CONFIG") {
        return (PathBuf::from(path), rig_id, remaining_args);
    }

    // 3. Fallback: ../launch.json relative to binary (macOS .app bundle layout)
    let exe = std::env::current_exe().expect("Failed to get current exe path");
    let contents_dir = exe
        .parent() // MacOS/
        .and_then(|p| p.parent()) // Contents/
        .expect("Invalid bundle structure: expected Contents/MacOS/REAPER");

    (contents_dir.join("launch.json"), rig_id, remaining_args)
}

/// Launch REAPER based on config discovered via CLI arg, env var, or bundle layout.
///
/// This is the main entry point for both wrapper `.app` binaries (macOS)
/// and wrapper scripts (Linux) that set `--config` or `FTS_LAUNCH_CONFIG`.
pub fn launch() -> ! {
    let (config_path, rig_id, extra_args) = discover_config();

    // Tell KDE/GNOME which .desktop file this window belongs to.
    // This lets the taskbar show the correct per-rig icon instead of
    // all REAPER instances sharing one icon via WM_CLASS.
    if let Some(ref id) = rig_id {
        // Safety: single-threaded at this point
        unsafe {
            std::env::set_var(
                "GIO_LAUNCHED_DESKTOP_FILE_PID",
                std::process::id().to_string(),
            );
            std::env::set_var("GIO_LAUNCHED_DESKTOP_FILE", format!("{id}.desktop"));
            // KDE-specific: set the desktop file name for window matching
            std::env::set_var("DESKTOP_FILE_ID", format!("{id}.desktop"));
        }
    }

    let config = match &rig_id {
        Some(id) => LaunchConfig::load_rig(&config_path, id).unwrap_or_else(|e| {
            panic!(
                "Failed to load rig '{id}' from {}: {e}",
                config_path.display()
            )
        }),
        None => LaunchConfig::load(&config_path).unwrap_or_else(|e| {
            panic!(
                "Failed to load launch config from {}: {e}",
                config_path.display()
            )
        }),
    };

    launch_with_config_and_args(&config, &extra_args);
}

/// Launch REAPER with the given config and no extra CLI args. Does not return.
pub fn launch_with_config(config: &LaunchConfig) -> ! {
    launch_with_config_and_args(config, &[]);
}

/// Launch REAPER with the given config and extra CLI args. Does not return.
pub fn launch_with_config_and_args(config: &LaunchConfig, extra_args: &[String]) -> ! {
    let ini = ReaperIni::new(&config.ini_path);

    // Build the list of ini patches
    let patches = config.ini_overrides.as_patches();
    let patch_refs: Vec<(&str, &str)> = patches.iter().map(|(k, v)| (*k, v.as_str())).collect();

    if config.restore_ini_after_launch && !patch_refs.is_empty() {
        // Fork: child launches REAPER, parent restores ini after a delay
        launch_with_restore(&ini, &patch_refs, config, extra_args);
    } else {
        // Simple path: patch and exec (no restore needed)
        if !patch_refs.is_empty()
            && let Err(e) = ini.patch(&patch_refs)
        {
            eprintln!("Warning: failed to patch reaper.ini: {e}");
        }
        exec_reaper(config, extra_args);
    }
}

/// Fork, patch ini, exec REAPER in child, restore ini in parent.
fn launch_with_restore(
    ini: &ReaperIni,
    patches: &[(&str, &str)],
    config: &LaunchConfig,
    extra_args: &[String],
) -> ! {
    // Save originals before patching
    let originals = ini.patch(patches).unwrap_or_else(|e| {
        eprintln!("Warning: failed to patch reaper.ini: {e}");
        std::collections::HashMap::new()
    });

    // Safety: we're about to fork. No threads should be running.
    let pid = unsafe { libc::fork() };

    match pid {
        -1 => {
            // Fork failed — just exec without restore
            eprintln!("Warning: fork failed, launching without ini restore");
            exec_reaper(config, extra_args);
        }
        0 => {
            // Child: exec REAPER
            exec_reaper(config, extra_args);
        }
        _parent => {
            // Parent: wait for REAPER to read its ini, then restore
            // 3 seconds is plenty for REAPER to parse reaper.ini on startup
            std::thread::sleep(std::time::Duration::from_secs(3));

            if let Err(e) = ini.restore(&originals) {
                eprintln!("Warning: failed to restore reaper.ini: {e}");
            }

            std::process::exit(0);
        }
    }
}

/// Set env vars and exec the real REAPER binary. Does not return.
fn exec_reaper(config: &LaunchConfig, extra_args: &[String]) -> ! {
    // Safety: we are single-threaded at this point (pre-exec), no other
    // threads are reading env vars concurrently.
    unsafe {
        std::env::set_var("FTS_DAW_ROLE", &config.role);
        if let Some(ref rig_type) = config.rig_type {
            std::env::set_var("FTS_RIG_TYPE", rig_type);
        }
    }

    std::env::set_current_dir(&config.resources_dir)
        .unwrap_or_else(|e| eprintln!("Warning: failed to chdir: {e}"));

    // If FTS_REAPER_FHS is set, exec REAPER through the FHS wrapper
    // so native libs (libGL, GDK, etc.) are available.
    let err = if let Ok(fhs) = std::env::var("FTS_REAPER_FHS") {
        Command::new(&fhs)
            .arg(&config.reaper_executable)
            .args(&config.reaper_args)
            .args(["-cfgfile", &config.ini_path])
            .args(extra_args)
            .exec()
    } else {
        Command::new(&config.reaper_executable)
            .args(&config.reaper_args)
            .args(["-cfgfile", &config.ini_path])
            .args(extra_args)
            .exec()
    };

    // exec() only returns on error
    panic!("Failed to exec REAPER: {err}");
}
