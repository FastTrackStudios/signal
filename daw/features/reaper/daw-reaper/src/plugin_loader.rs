//! REAPER Plugin Loader Implementation
//!
//! Eagerly loads REAPER plugins (.clap, .vst3) by calling `ReaperPluginEntry`
//! on dynamically loaded libraries. Each loaded plugin gets its own copy of
//! Rust statics (reaper-high, TaskSupport, daw-reaper).
//!
//! # Safety
//!
//! Plugin loading involves `dlopen` and calling C FFI entry points. The caller
//! must ensure plugin binaries are trusted.

use daw_proto::plugin_loader::{LoadedPluginInfo, PluginLoadResult, PluginLoading};
use std::sync::Mutex;
use tracing::{debug, info, warn};

/// Stored plugin context for calling `ReaperPluginEntry` on loaded plugins.
///
/// Must be initialized via [`set_plugin_context`] before any plugins can be loaded.
struct PluginContextData {
    h_instance: reaper_low::raw::HINSTANCE,
    raw_info: reaper_low::raw::reaper_plugin_info_t,
}

// SAFETY: The raw pointers in HINSTANCE and reaper_plugin_info_t are stable
// for the process lifetime (set once during extension init, never moved).
unsafe impl Send for PluginContextData {}
unsafe impl Sync for PluginContextData {}

static PLUGIN_CONTEXT: std::sync::OnceLock<PluginContextData> = std::sync::OnceLock::new();
static LOADED_PLUGINS: std::sync::OnceLock<Mutex<Vec<LoadedEntry>>> = std::sync::OnceLock::new();

struct LoadedEntry {
    path: String,
    name: String,
    #[allow(dead_code)]
    library: libloading::Library,
}

fn loaded_plugins() -> &'static Mutex<Vec<LoadedEntry>> {
    LOADED_PLUGINS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Store the REAPER plugin context for later use by the loader.
///
/// Call this from `plugin_main` before any plugin loading can happen.
pub fn set_plugin_context(context: reaper_low::PluginContext) {
    let ext_context = match context.type_specific() {
        reaper_low::TypeSpecificPluginContext::Extension(ext) => ext,
        _ => {
            warn!("Cannot set plugin context: not an extension context");
            return;
        }
    };

    let raw_info = ext_context.to_raw();
    let h_instance = context.h_instance();

    let _ = PLUGIN_CONTEXT.set(PluginContextData {
        h_instance,
        raw_info,
    });
    debug!("Plugin loader context stored");
}

fn derive_plugin_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// REAPER implementation of the PluginLoaderService.
#[derive(Clone)]
pub struct ReaperPluginLoader;

impl ReaperPluginLoader {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReaperPluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Scan `UserPlugins/FX/` for `.clap` files that export `ReaperPluginEntry`
/// and eagerly load them. This gives FTS CLAP plugins direct REAPER API access
/// (timers, track queries, etc.) without each extension having to load them manually.
///
/// Plugins that don't export the symbol are silently skipped — this is expected
/// for third-party CLAP plugins.
///
/// Must be called after [`set_plugin_context`] and after REAPER's high-level API
/// is initialized.
pub fn eager_load_fx_plugins() {
    let ctx = match PLUGIN_CONTEXT.get() {
        Some(ctx) => ctx,
        None => {
            warn!("eager_load_fx_plugins: plugin context not set, skipping");
            return;
        }
    };

    let reaper = reaper_high::Reaper::get();
    let resource_path = reaper
        .medium_reaper()
        .get_resource_path(|p| p.to_path_buf());
    let fx_dir = resource_path
        .into_std_path_buf()
        .join("UserPlugins")
        .join("FX")
        .join("FTS");

    if !fx_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&fx_dir) {
            warn!("Failed to create UserPlugins/FX/FTS/: {e}");
            return;
        }
        info!("Created UserPlugins/FX/FTS/ directory");
    }

    let entries = match std::fs::read_dir(&fx_dir) {
        Ok(e) => e,
        Err(e) => {
            warn!("Failed to read UserPlugins/FX/: {e}");
            return;
        }
    };

    type EntryFn = unsafe extern "C" fn(
        reaper_low::raw::HINSTANCE,
        *mut reaper_low::raw::reaper_plugin_info_t,
    ) -> std::os::raw::c_int;

    for entry in entries.flatten() {
        let path = entry.path();

        // Follow symlinks to get the real file
        let real_path = match std::fs::canonicalize(&path) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Only process .clap files (on Linux these are .so files renamed to .clap,
        // but our bundler may produce either — check extension of the symlink name)
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "clap" && ext != "so" && ext != "dylib" {
            continue;
        }

        let path_str = real_path.to_string_lossy().to_string();

        // Skip if already loaded
        if let Ok(plugins) = loaded_plugins().lock()
            && plugins.iter().any(|p| p.path == path_str)
        {
            continue;
        }

        // Try to dlopen and check for FtsReaperInit (preferred) or ReaperPluginEntry (legacy)
        let lib = match unsafe { libloading::Library::new(&real_path) } {
            Ok(lib) => lib,
            Err(_) => continue,
        };

        // Only eager-load plugins that export FtsReaperInit — this avoids
        // interfering with REAPER's own CLAP scanner which handles clap_entry.
        let entry_fn = match unsafe { lib.get::<EntryFn>(b"FtsReaperInit\0") } {
            Ok(f) => f,
            Err(_) => continue, // No FtsReaperInit — let REAPER handle it normally
        };

        let name = derive_plugin_name(&path_str);
        info!("Eager-loading FX plugin: {name} ({path_str})");

        let mut raw_info = ctx.raw_info;
        let result = unsafe { entry_fn(ctx.h_instance, &mut raw_info as *mut _) };

        if result == 0 {
            warn!("Plugin {name}: ReaperPluginEntry returned 0 (init failed)");
            // Don't keep the library — drop it
            continue;
        }

        info!("Plugin eager-loaded successfully: {name}");

        // Keep the library alive for the process lifetime
        if let Ok(mut plugins) = loaded_plugins().lock() {
            plugins.push(LoadedEntry {
                path: path_str,
                name,
                library: lib,
            });
        }
    }
}

impl PluginLoading for crate::Reaper {
    fn load(&self, plugin_path: &str) -> PluginLoadResult {
        let plugin_path = plugin_path.to_string();
        // Check if already loaded
        if let Ok(plugins) = loaded_plugins().lock()
            && plugins.iter().any(|p| p.path == plugin_path)
        {
            return PluginLoadResult::AlreadyLoaded;
        }

        let ctx = match PLUGIN_CONTEXT.get() {
            Some(ctx) => ctx,
            None => {
                return PluginLoadResult::Error(
                    "Plugin context not initialized — set_plugin_context() not called".to_string(),
                );
            }
        };

        let path = std::path::Path::new(&plugin_path);
        if !path.exists() {
            return PluginLoadResult::Error(format!("Plugin not found: {plugin_path}"));
        }

        info!("Loading plugin from: {plugin_path}");

        // dlopen the plugin binary
        let lib = match unsafe { libloading::Library::new(path) } {
            Ok(lib) => lib,
            Err(e) => {
                return PluginLoadResult::Error(format!("Failed to load library: {e}"));
            }
        };

        // Look up ReaperPluginEntry
        type EntryFn = unsafe extern "C" fn(
            reaper_low::raw::HINSTANCE,
            *mut reaper_low::raw::reaper_plugin_info_t,
        ) -> std::os::raw::c_int;

        let entry_fn = match unsafe { lib.get::<EntryFn>(b"ReaperPluginEntry\0") } {
            Ok(f) => f,
            Err(e) => {
                return PluginLoadResult::Error(format!("ReaperPluginEntry symbol not found: {e}"));
            }
        };

        // Call the plugin's entry point with our stored context
        let mut raw_info = ctx.raw_info;
        let result = unsafe { entry_fn(ctx.h_instance, &mut raw_info as *mut _) };

        if result == 0 {
            warn!("Plugin {plugin_path}: ReaperPluginEntry returned 0 (init failed)");
            return PluginLoadResult::Error("ReaperPluginEntry returned 0".to_string());
        }

        let name = derive_plugin_name(&plugin_path);
        info!("Plugin loaded successfully: {name} (result={result})");

        // Keep the library alive
        if let Ok(mut plugins) = loaded_plugins().lock() {
            plugins.push(LoadedEntry {
                path: plugin_path,
                name,
                library: lib,
            });
        }

        PluginLoadResult::Ok
    }

    fn list_loaded(&self) -> Vec<LoadedPluginInfo> {
        loaded_plugins()
            .lock()
            .ok()
            .map(|plugins| {
                plugins
                    .iter()
                    .map(|p| LoadedPluginInfo {
                        path: p.path.clone(),
                        name: p.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_loaded(&self, plugin_path: &str) -> bool {
        loaded_plugins()
            .lock()
            .ok()
            .map(|plugins| plugins.iter().any(|p| p.path == plugin_path))
            .unwrap_or(false)
    }
}
