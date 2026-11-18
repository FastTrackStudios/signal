//! `FxRigBinding` — discover rig structure from a DAW track's FX chain.
//!
//! Scans an FX chain (via `DawBridge`) and maps FX plugins to module slots.
//! This bridges the gap between "what the DAW has" and "what signal models."

use signal_proto::module_type::ModuleType;
use std::collections::HashMap;

/// A discovered FX plugin in the DAW chain.
#[derive(Debug, Clone)]
pub struct DiscoveredFx {
    /// FX identifier (GUID or index-based).
    pub fx_id: String,
    /// Plugin name as reported by the DAW.
    pub plugin_name: String,
    /// FX index in the chain.
    pub chain_index: u32,
    /// Whether this FX is inside a container.
    pub container_id: Option<String>,
}

/// A discovered module — a group of FX mapped to a module type.
#[derive(Debug, Clone)]
pub struct DiscoveredModule {
    pub module_type: ModuleType,
    /// FX plugins belonging to this module.
    pub fx_list: Vec<DiscoveredFx>,
    /// Whether the module is currently enabled (not bypassed).
    pub enabled: bool,
}

/// The full discovered rig structure from an FX chain scan.
#[derive(Debug, Clone)]
pub struct DiscoveredRig {
    /// Track identifier.
    pub track_id: String,
    /// Discovered modules in chain order.
    pub modules: Vec<DiscoveredModule>,
}

/// Binding state tracking how a signal rig maps to DAW FX.
#[derive(Debug)]
pub struct FxRigBinding {
    /// Track this binding is attached to.
    track_id: String,
    /// Discovered rig structure.
    discovered: Option<DiscoveredRig>,
    /// Module type → FX ID mapping for parameter access.
    fx_map: HashMap<ModuleType, Vec<String>>,
}

impl FxRigBinding {
    /// Create a new binding for a track (not yet scanned).
    pub fn new(track_id: impl Into<String>) -> Self {
        Self {
            track_id: track_id.into(),
            discovered: None,
            fx_map: HashMap::new(),
        }
    }

    /// The track this binding is attached to.
    #[must_use]
    pub fn track_id(&self) -> &str {
        &self.track_id
    }

    /// Whether the FX chain has been scanned.
    #[must_use]
    pub const fn is_bound(&self) -> bool {
        self.discovered.is_some()
    }

    /// Set the discovered rig structure (after scanning the FX chain).
    pub fn bind(&mut self, rig: DiscoveredRig) {
        self.fx_map.clear();
        for module in &rig.modules {
            let fx_ids: Vec<String> = module.fx_list.iter().map(|fx| fx.fx_id.clone()).collect();
            self.fx_map.insert(module.module_type, fx_ids);
        }
        self.discovered = Some(rig);
    }

    /// Clear the binding (e.g., when the track is removed).
    pub fn unbind(&mut self) {
        self.discovered = None;
        self.fx_map.clear();
    }

    /// Get the discovered rig structure.
    #[must_use]
    pub const fn discovered_rig(&self) -> Option<&DiscoveredRig> {
        self.discovered.as_ref()
    }

    /// Get FX IDs for a module type.
    #[must_use]
    pub fn fx_ids_for_module(&self, module_type: ModuleType) -> &[String] {
        self.fx_map
            .get(&module_type)
            .map_or(&[], std::vec::Vec::as_slice)
    }

    /// All module types that are bound.
    #[must_use]
    pub fn bound_module_types(&self) -> Vec<ModuleType> {
        self.fx_map.keys().copied().collect()
    }

    /// Number of bound modules.
    #[must_use]
    pub fn module_count(&self) -> usize {
        self.fx_map.len()
    }
}
