//! Tiny key/value prefs — one storage API for every surface the app
//! ships on. Native keeps files under ~/.config/fts (one file per key,
//! shared with the CLI and the engine tooling); the web build keeps the
//! same keys in localStorage under `fts.<key>`.

#[cfg(not(target_arch = "wasm32"))]
fn config_path(key: &str) -> Option<std::path::PathBuf> {
    // Honor XDG_CONFIG_HOME — on iOS the app roots it under
    // Documents/FastTrackStudio (the container's ~/.config isn't writable).
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(std::path::Path::new(&xdg).join("fts").join(key));
    }
    let home = std::env::var_os("HOME")?;
    Some(std::path::Path::new(&home).join(".config/fts").join(key))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn get(key: &str) -> Option<String> {
    let val = std::fs::read_to_string(config_path(key)?).ok()?;
    let val = val.trim().to_string();
    (!val.is_empty()).then_some(val)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn set(key: &str, value: &str) {
    let Some(path) = config_path(key) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, value);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remove(key: &str) {
    if let Some(path) = config_path(key) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn get(key: &str) -> Option<String> {
    let val = local_storage()?.get_item(&format!("fts.{key}")).ok()??;
    let val = val.trim().to_string();
    (!val.is_empty()).then_some(val)
}

#[cfg(target_arch = "wasm32")]
pub fn set(key: &str, value: &str) {
    if let Some(s) = local_storage() {
        let _ = s.set_item(&format!("fts.{key}"), value);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn remove(key: &str) {
    if let Some(s) = local_storage() {
        let _ = s.remove_item(&format!("fts.{key}"));
    }
}
