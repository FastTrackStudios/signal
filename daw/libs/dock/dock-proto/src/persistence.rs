//! Persistence — save/load dock presets and layouts to JSON.
//!
//! Provides helpers for serializing the entire preset collection
//! and individual layouts to JSON strings or files.

use crate::layout::DockLayout;
use crate::preset::PresetCollection;
use crate::tree::DockNode;
use crate::workspace::DockWorkspaceState;
use crate::{DockWindow, PresetId, WindowId};
use std::collections::HashMap;

#[derive(facet::Facet)]
struct PersistedDockPreset {
    id: PresetId,
    name: String,
    layout: Option<DockNode>,
    hotkey_hint: Option<String>,
    is_builtin: bool,
    panel_states: Option<HashMap<String, String>>,
}

#[derive(facet::Facet)]
struct PersistedPresetCollection {
    presets: Vec<PersistedDockPreset>,
    active_index: usize,
}

#[derive(facet::Facet)]
struct PersistedWindow {
    id: WindowId,
    layout: Option<DockNode>,
    title: String,
    bounds: crate::workspace::WindowBounds,
    is_main: bool,
}

#[derive(facet::Facet)]
struct PersistedWorkspaceState {
    windows: Vec<PersistedWindow>,
    main_window: WindowId,
}

/// Serialize a preset collection to a pretty-printed JSON string.
pub fn presets_to_json(presets: &PresetCollection) -> Result<String, Box<dyn std::error::Error>> {
    let persisted = PersistedPresetCollection {
        presets: presets
            .presets
            .iter()
            .map(|p| PersistedDockPreset {
                id: p.id,
                name: p.name.clone(),
                layout: p.layout.to_tree(),
                hotkey_hint: p.hotkey_hint.clone(),
                is_builtin: p.is_builtin,
                panel_states: Some(p.panel_states.clone()),
            })
            .collect(),
        active_index: presets.active_index,
    };
    Ok(facet_json::to_string_pretty(&persisted)?)
}

/// Deserialize a preset collection from a JSON string.
pub fn presets_from_json(json: &str) -> Result<PresetCollection, Box<dyn std::error::Error>> {
    let persisted: PersistedPresetCollection = facet_json::from_str(json)?;
    Ok(PresetCollection {
        presets: persisted
            .presets
            .into_iter()
            .map(|p| crate::preset::DockPreset {
                id: p.id,
                name: p.name,
                layout: p
                    .layout
                    .map_or_else(DockLayout::empty, DockLayout::from_tree),
                hotkey_hint: p.hotkey_hint,
                is_builtin: p.is_builtin,
                panel_states: p.panel_states.unwrap_or_default(),
            })
            .collect(),
        active_index: persisted.active_index,
    })
}

/// Serialize a single layout to a JSON string.
pub fn layout_to_json(layout: &DockLayout) -> Result<String, Box<dyn std::error::Error>> {
    Ok(facet_json::to_string_pretty(&layout.to_tree())?)
}

/// Deserialize a single layout from a JSON string.
pub fn layout_from_json(json: &str) -> Result<DockLayout, Box<dyn std::error::Error>> {
    let tree: Option<DockNode> = facet_json::from_str(json)?;
    Ok(tree.map_or_else(DockLayout::empty, DockLayout::from_tree))
}

/// Serialize workspace state (all windows) to JSON.
pub fn workspace_to_json(
    workspace: &DockWorkspaceState,
) -> Result<String, Box<dyn std::error::Error>> {
    let persisted = PersistedWorkspaceState {
        windows: workspace
            .windows
            .values()
            .map(|w| PersistedWindow {
                id: w.id,
                layout: w.layout.to_tree(),
                title: w.title.clone(),
                bounds: w.bounds.clone(),
                is_main: w.is_main,
            })
            .collect(),
        main_window: workspace.main_window,
    };
    Ok(facet_json::to_string_pretty(&persisted)?)
}

/// Deserialize workspace state (all windows) from JSON.
pub fn workspace_from_json(json: &str) -> Result<DockWorkspaceState, Box<dyn std::error::Error>> {
    let persisted: PersistedWorkspaceState = facet_json::from_str(json)?;
    let windows = persisted
        .windows
        .into_iter()
        .map(|w| {
            (
                w.id,
                DockWindow {
                    id: w.id,
                    layout: w
                        .layout
                        .map_or_else(DockLayout::empty, DockLayout::from_tree),
                    title: w.title,
                    bounds: w.bounds,
                    is_main: w.is_main,
                },
            )
        })
        .collect();

    Ok(DockWorkspaceState {
        windows,
        main_window: persisted.main_window,
    })
}

/// Save presets to a file. Creates parent directories if needed.
pub fn save_presets_to_file(
    presets: &PresetCollection,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = presets_to_json(presets)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json)?;
    Ok(())
}

/// Load presets from a file. Returns None if the file doesn't exist.
pub fn load_presets_from_file(
    path: &std::path::Path,
) -> Result<Option<PresetCollection>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(path)?;
    let presets = presets_from_json(&json)?;
    Ok(Some(presets))
}

/// Get the default presets file path for the application.
///
/// Returns `~/.config/fts-control/dock-presets.json` on Unix
/// or `%APPDATA%/fts-control/dock-presets.json` on Windows.
pub fn default_presets_path() -> Option<std::path::PathBuf> {
    dirs_or_home().map(|dir| dir.join("dock-presets.json"))
}

fn dirs_or_home() -> Option<std::path::PathBuf> {
    // Try XDG config dir first, fall back to home
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(std::path::PathBuf::from(config).join("fts-control"));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(
            std::path::PathBuf::from(home)
                .join(".config")
                .join("fts-control"),
        );
    }
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return Some(std::path::PathBuf::from(appdata).join("fts-control"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::default_presets;
    use crate::{DockWorkspace, PanelId, PanelRegistry, WindowBounds};

    #[test]
    fn json_roundtrip() {
        let presets = default_presets();
        let json = presets_to_json(&presets).unwrap();
        let restored = presets_from_json(&json).unwrap();
        assert_eq!(presets.presets.len(), restored.presets.len());
        for (orig, rest) in presets.presets.iter().zip(restored.presets.iter()) {
            assert_eq!(orig.name, rest.name);
            assert_eq!(orig.layout.node_count(), rest.layout.node_count());
        }
    }

    #[test]
    fn file_roundtrip() {
        let dir = std::env::temp_dir().join("dock_proto_test");
        let path = dir.join("test-presets.json");

        let presets = default_presets();
        save_presets_to_file(&presets, &path).unwrap();

        let loaded = load_presets_from_file(&path).unwrap().unwrap();
        assert_eq!(loaded.presets.len(), presets.presets.len());

        // Cleanup
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_nonexistent_returns_none() {
        let path = std::path::Path::new("/tmp/dock_proto_test_nonexistent.json");
        let result = load_presets_from_file(path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn workspace_json_roundtrip() {
        let mut registry = PanelRegistry::new();
        PanelId::register_all(&mut registry);
        let ws = DockWorkspace::with_main_window(
            DockLayout::single(PanelId::Performance),
            "Main",
            WindowBounds::new(0.0, 0.0, 1200.0, 800.0, "main"),
            registry,
        );

        let json = workspace_to_json(&ws.to_state()).unwrap();
        let restored = workspace_from_json(&json).unwrap();
        assert_eq!(restored.windows.len(), 1);
        assert!(restored.windows.contains_key(&restored.main_window));
    }

    #[test]
    fn preset_panel_states_roundtrip_and_backward_compat() {
        let mut presets = default_presets();
        presets.active_index = 0;
        presets.save_active_panel_state("panel.performance", r#"{"scroll":42,"zoom":1.25}"#);

        let json = presets_to_json(&presets).unwrap();
        let restored = presets_from_json(&json).unwrap();
        assert_eq!(
            restored
                .presets
                .first()
                .and_then(|p| p.load_panel_state("panel.performance")),
            Some(r#"{"scroll":42,"zoom":1.25}"#),
        );

        // Older payload without panel_states must still deserialize.
        let legacy_source = presets_to_json(&default_presets()).unwrap();
        let legacy = legacy_source
            .replacen(r#","panel_states":{}"#, "", 1)
            .replacen(r#""panel_states":{},"#, "", 1);
        let legacy_restored = presets_from_json(&legacy).unwrap();
        assert_eq!(
            legacy_restored.presets.len(),
            default_presets().presets.len()
        );
        assert_eq!(
            legacy_restored.presets[0].load_panel_state("panel.performance"),
            None
        );
    }
}
