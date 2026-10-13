//! FTS Launcher — Dioxus-native launcher for Reaper.
//!
//! This crate provides the full launcher: engine, UI components, providers,
//! and Dioxus rendering via the Blitz native renderer.
//!
//! # Embedding in a Reaper extension
//!
//! ```rust,ignore
//! // In your extension's plugin_main:
//! let engine = fts_launcher::LauncherEngine::new();
//!
//! // Register launcher actions with REAPER
//! for (id, name, handler) in engine.action_defs() { ... }
//!
//! // The Launcher Dioxus component can be embedded in a reaper-dioxus panel:
//! use fts_launcher::ui::{Launcher, LauncherState};
//! let state = engine.into_state();
//! // Render: Launcher { state, on_close: ... }
//! ```

pub mod daw_module;
pub mod reaper;

use std::sync::Arc;

use architect_launcher_core::{FilterState, QueryEngine};
use architect_launcher_providers::{CalcProvider, ExtensionProvider, WorkflowProvider};

// Re-export everything consumers need
pub use architect_launcher_core;
pub use architect_launcher_ui;
pub use dioxus_native;

/// Re-export the UI components under a convenient path.
pub mod ui {
    pub use architect_launcher_ui::components::Launcher;
    pub use architect_launcher_ui::state::LauncherState;
}

/// Action handler type: (command_id, display_name, handler).
pub type ActionDef = (String, String, Arc<dyn Fn() + Send + Sync>);

/// The launcher engine — holds the query engine, providers, and configuration.
pub struct LauncherEngine {
    engine: QueryEngine,
    _ext_registry: architect_launcher_core::ExtensionRegistry,
}

impl LauncherEngine {
    /// Create a new launcher engine, loading packs and extensions.
    pub fn new() -> Self {
        // Load workflow packs
        let mut loaded_packs = Vec::new();
        let bundled_dir = std::path::PathBuf::from(
            std::env::var("FTS_LAUNCHER_PACKS").unwrap_or_else(|_| "packs".into()),
        );
        loaded_packs.extend(architect_launcher_core::pack::scan_packs(&bundled_dir));
        let user_dir = architect_launcher_core::pack::default_pack_dir();
        loaded_packs.extend(architect_launcher_core::pack::scan_packs(&user_dir));
        tracing::info!(packs = loaded_packs.len(), "Loaded workflow packs");

        // Load extensions
        let mut ext_registry = architect_launcher_core::ExtensionRegistry::new();
        let bundled_ext = std::path::PathBuf::from(
            std::env::var("FTS_LAUNCHER_EXTENSIONS").unwrap_or_else(|_| "extensions".into()),
        );
        ext_registry.scan_dir(&bundled_ext);
        let user_ext = architect_launcher_core::extension::default_extensions_dir();
        ext_registry.scan_dir(&user_ext);
        tracing::info!(
            extensions = ext_registry.extensions().len(),
            "Loaded extensions"
        );

        let engine = QueryEngine::builder()
            .max_results(50)
            .register_tags(|tags| {
                reaper::register_reaper_tags(tags);
            })
            .register_packs(&loaded_packs)
            .register_tags(|tags| {
                ext_registry.register_tags(tags);
            })
            .magic_word("C", "Compressors")
            .magic_word("R", "Reverbs")
            .magic_word("E", "EQ")
            .magic_word("I", "Instruments")
            .magic_word("A", "Actions")
            .magic_word("V", "Visibility")
            .magic_word("T", "Tracks")
            .provider(Box::new(reaper::DawTracksProvider::new()))
            .provider(Box::new(reaper::DawFxProvider::new()))
            .provider(Box::new(reaper::DawActionsProvider::new()))
            .provider(Box::new(reaper::DawMarkersProvider::new()))
            .provider(Box::new(reaper::DawTransportProvider::new()))
            .provider(Box::new(CalcProvider::new()))
            .provider(Box::new(WorkflowProvider::from_items(
                loaded_packs.iter().flat_map(|p| p.to_items()).collect(),
            )))
            .provider(Box::new(ExtensionProvider::from_registry(&ext_registry)))
            .build();

        // Register pack presets
        for pack in &loaded_packs {
            for preset in &pack.def.presets {
                if engine.load_preset(&preset.name).is_none() {
                    engine.save_preset(
                        &preset.name,
                        FilterState {
                            include: preset.include.clone(),
                            exclude: preset.exclude.clone(),
                            ..Default::default()
                        },
                    );
                }
            }
        }

        // Seed default presets
        for (name, include) in [
            ("Compressors", vec!["audio/effects/dynamics"]),
            ("Reverbs", vec!["audio/effects/reverb"]),
            ("EQ", vec!["audio/effects/eq"]),
            ("Instruments", vec!["audio/instruments"]),
            ("Actions", vec!["reaper/actions"]),
            ("Tracks", vec!["reaper/tracks"]),
        ] {
            if engine.load_preset(name).is_none() {
                engine.save_preset(
                    name,
                    FilterState {
                        include: include.into_iter().map(|s| s.to_string()).collect(),
                        ..Default::default()
                    },
                );
            }
        }

        tracing::info!(
            providers = engine.provider_names().len(),
            "FTS Launcher ready"
        );

        Self {
            engine,
            _ext_registry: ext_registry,
        }
    }

    /// Get the query engine.
    pub fn engine(&self) -> &QueryEngine {
        &self.engine
    }

    /// Consume into the query engine.
    pub fn into_engine(self) -> QueryEngine {
        self.engine
    }

    /// Create a LauncherState for the Dioxus UI.
    pub fn into_state(self) -> ui::LauncherState {
        ui::LauncherState::new(self.engine)
    }

    /// Action definitions for REAPER registration.
    pub fn action_defs() -> Vec<ActionDef> {
        vec![
            (
                "FTS_LAUNCHER_TOGGLE".to_string(),
                "FTS: Toggle Launcher".to_string(),
                Arc::new(|| {
                    tracing::info!("FTS Launcher toggle");
                }),
            ),
            (
                "FTS_LAUNCHER_FOCUS".to_string(),
                "FTS: Focus Launcher Search".to_string(),
                Arc::new(|| {
                    tracing::info!("FTS Launcher focus");
                }),
            ),
        ]
    }
}

impl Default for LauncherEngine {
    fn default() -> Self {
        Self::new()
    }
}
