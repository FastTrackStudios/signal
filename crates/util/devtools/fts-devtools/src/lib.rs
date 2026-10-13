//! Shared dev tooling for FTS repos.
//!
//! Provides helpers for installing (symlinking) REAPER plugins and integrated
//! extensions into the correct directories. Each repo's xtask or build script
//! can use these instead of hand-rolling install logic.
//!
//! # REAPER directory layout
//!
//! ```text
//! ~/.config/REAPER/                          (or custom REAPER_PATH)
//! └── UserPlugins/
//!     ├── reaper_daw_bridge.so               ← socket bridge plugin
//!     └── reaper_example_extension.so        ← integrated extension plugin
//! ```

use std::path::{Path, PathBuf};

/// Errors from install operations.
#[derive(Debug)]
pub enum InstallError {
    /// The source binary doesn't exist.
    SourceNotFound(PathBuf),
    /// Failed to create target directory.
    CreateDir(PathBuf, std::io::Error),
    /// Failed to remove existing file/symlink.
    Remove(PathBuf, std::io::Error),
    /// Failed to create symlink (or copy on Windows).
    Link(PathBuf, PathBuf, std::io::Error),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceNotFound(p) => write!(f, "source binary not found: {}", p.display()),
            Self::CreateDir(p, e) => write!(f, "failed to create {}: {e}", p.display()),
            Self::Remove(p, e) => write!(f, "failed to remove {}: {e}", p.display()),
            Self::Link(src, dst, e) => {
                write!(
                    f,
                    "failed to link {} -> {}: {e}",
                    dst.display(),
                    src.display()
                )
            }
        }
    }
}

impl std::error::Error for InstallError {}

/// Discover REAPER config directories.
///
/// Returns all directories where UserPlugins should be installed.
/// Checks `REAPER_PATH` env var first, then falls back to the standard
/// locations on each platform.
pub fn reaper_dirs() -> Vec<PathBuf> {
    // Explicit override
    if let Ok(path) = std::env::var("REAPER_PATH") {
        return vec![PathBuf::from(path)];
    }

    let home = match std::env::var("HOME") {
        Ok(h) => PathBuf::from(h),
        Err(_) => return vec![],
    };

    let mut dirs = vec![];

    #[cfg(target_os = "linux")]
    {
        let r1 = home.join(".config/REAPER");
        // Real install (canonical); the old ~/.config/FastTrackStudio/Reaper
        // is a compat symlink to it on migrated machines.
        let r2 = home.join("fasttrackstudio");
        let r2_legacy = home.join(".config/FastTrackStudio/Reaper");
        if r1.is_dir() {
            dirs.push(r1);
        }
        if r2.is_dir() {
            dirs.push(r2);
        } else if r2_legacy.is_dir() {
            dirs.push(r2_legacy);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let r = home.join("Library/Application Support/REAPER");
        if r.is_dir() {
            dirs.push(r);
        }
    }

    dirs
}

/// Symlink (or copy on Windows) a file to a destination.
///
/// Removes any existing file/symlink at `dest` first.
fn force_link(src: &Path, dest: &Path) -> Result<(), InstallError> {
    // Remove existing
    if dest.exists() || dest.is_symlink() {
        std::fs::remove_file(dest).map_err(|e| InstallError::Remove(dest.to_path_buf(), e))?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dest)
        .map_err(|e| InstallError::Link(src.to_path_buf(), dest.to_path_buf(), e))?;

    #[cfg(not(unix))]
    std::fs::copy(src, dest)
        .map(|_| ())
        .map_err(|e| InstallError::Link(src.to_path_buf(), dest.to_path_buf(), e))?;

    Ok(())
}

/// Install a REAPER plugin (.so/.dylib) into UserPlugins.
///
/// `lib_path` is the built library (e.g. `target/debug/libreaper_daw_bridge.so`).
/// `lib_name` is the filename in UserPlugins (e.g. `reaper_daw_bridge.so`).
///
/// Installs into all discovered REAPER directories.
pub fn install_plugin(lib_path: &Path, lib_name: &str) -> Result<(), InstallError> {
    if !lib_path.exists() {
        return Err(InstallError::SourceNotFound(lib_path.to_path_buf()));
    }

    let lib_path = lib_path
        .canonicalize()
        .unwrap_or_else(|_| lib_path.to_path_buf());

    for reaper_dir in reaper_dirs() {
        let user_plugins = reaper_dir.join("UserPlugins");
        std::fs::create_dir_all(&user_plugins)
            .map_err(|e| InstallError::CreateDir(user_plugins.clone(), e))?;

        let dest = user_plugins.join(lib_name);
        force_link(&lib_path, &dest)?;
        println!("  installed: {} -> {}", dest.display(), lib_path.display());
    }

    Ok(())
}

/// Install an integrated REAPER extension into `UserPlugins/`.
///
/// `binary_path` is the built library (for example,
/// `target/debug/libreaper_signal.so`). `name` is the filename REAPER should
/// load from `UserPlugins` (for example, `"reaper_signal.so"`).
///
/// Installs into all discovered REAPER directories.
pub fn install_extension(binary_path: &Path, name: &str) -> Result<(), InstallError> {
    if !binary_path.exists() {
        return Err(InstallError::SourceNotFound(binary_path.to_path_buf()));
    }

    let binary_path = binary_path
        .canonicalize()
        .unwrap_or_else(|_| binary_path.to_path_buf());

    for reaper_dir in reaper_dirs() {
        let ext_dir = reaper_dir.join("UserPlugins");
        std::fs::create_dir_all(&ext_dir)
            .map_err(|e| InstallError::CreateDir(ext_dir.clone(), e))?;

        let dest = ext_dir.join(name);
        force_link(&binary_path, &dest)?;
        println!(
            "  installed: {} -> {}",
            dest.display(),
            binary_path.display()
        );
    }

    Ok(())
}

/// Uninstall an integrated REAPER extension from `UserPlugins/`.
pub fn uninstall_extension(name: &str) {
    for reaper_dir in reaper_dirs() {
        let dest = reaper_dir.join("UserPlugins").join(name);
        if dest.exists() || dest.is_symlink() {
            let _ = std::fs::remove_file(&dest);
            println!("  removed: {}", dest.display());
        }
    }
}

/// Show the status of all installed extensions and plugins.
pub fn status() {
    let dirs = reaper_dirs();
    if dirs.is_empty() {
        println!("No REAPER installations found.");
        return;
    }

    for reaper_dir in dirs {
        println!("=== {} ===", reaper_dir.display());

        let user_plugins = reaper_dir.join("UserPlugins");
        if user_plugins.is_dir() {
            println!("  UserPlugins:");
            if let Ok(entries) = std::fs::read_dir(&user_plugins) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if (path.is_file() || path.is_symlink())
                        && let Some(name) = path.file_name()
                    {
                        let name = name.to_string_lossy();
                        if name.contains("reaper_")
                            || name.ends_with(".so")
                            || name.ends_with(".dylib")
                        {
                            print_link_status(&path);
                        }
                    }
                }
            }
        }
        println!();
    }
}

fn print_link_status(path: &Path) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    if path.is_symlink() {
        match std::fs::read_link(path) {
            Ok(target) => {
                let ok = target.exists();
                let marker = if ok { "ok" } else { "BROKEN" };
                println!("    {name} -> {} [{marker}]", target.display());
            }
            Err(_) => println!("    {name} [symlink, unreadable]"),
        }
    } else if path.exists() {
        println!("    {name} [file]");
    } else {
        println!("    {name} [missing]");
    }
}
