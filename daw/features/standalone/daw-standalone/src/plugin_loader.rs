//! `impl PluginLoading for Standalone`.

use daw_proto::plugin_loader::{LoadedPluginInfo, PluginLoadResult, PluginLoading};

use crate::sync::Standalone;

fn name_from_path(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

impl PluginLoading for Standalone {
    fn load(&self, path: &str) -> PluginLoadResult {
        {
            let loaded = self.loaded_plugins.lock().expect("loaded_plugins poisoned");
            if loaded.contains_key(path) {
                return PluginLoadResult::AlreadyLoaded;
            }
        }

        match crate::plugin::load_plugin(path) {
            Ok(Some(_plugin)) => {
                let mut loaded = self.loaded_plugins.lock().expect("loaded_plugins poisoned");
                loaded.insert(
                    path.to_string(),
                    LoadedPluginInfo {
                        path: path.to_string(),
                        name: name_from_path(path),
                    },
                );
                PluginLoadResult::Ok
            }
            Ok(None) => {
                PluginLoadResult::Error(format!("not a hostable CLAP/VST3 plugin path: {path}"))
            }
            Err(e) => PluginLoadResult::Error(e.to_string()),
        }
    }

    fn list_loaded(&self) -> Vec<LoadedPluginInfo> {
        let loaded = self.loaded_plugins.lock().expect("loaded_plugins poisoned");
        loaded.values().cloned().collect()
    }

    fn is_loaded(&self, path: &str) -> bool {
        let loaded = self.loaded_plugins.lock().expect("loaded_plugins poisoned");
        loaded.contains_key(path)
    }
}
