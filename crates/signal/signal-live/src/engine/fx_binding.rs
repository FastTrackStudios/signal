//! FxRigBinding — discover rig structure from a DAW track's FX chain.
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
    pub fn track_id(&self) -> &str {
        &self.track_id
    }

    /// Whether the FX chain has been scanned.
    pub fn is_bound(&self) -> bool {
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
    pub fn discovered_rig(&self) -> Option<&DiscoveredRig> {
        self.discovered.as_ref()
    }

    /// Get FX IDs for a module type.
    pub fn fx_ids_for_module(&self, module_type: ModuleType) -> &[String] {
        self.fx_map
            .get(&module_type)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// All module types that are bound.
    pub fn bound_module_types(&self) -> Vec<ModuleType> {
        self.fx_map.keys().copied().collect()
    }

    /// Number of bound modules.
    pub fn module_count(&self) -> usize {
        self.fx_map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fx(id: &str, name: &str, idx: u32) -> DiscoveredFx {
        DiscoveredFx {
            fx_id: id.into(),
            plugin_name: name.into(),
            chain_index: idx,
            container_id: None,
        }
    }

    #[test]
    fn new_binding_is_unbound() {
        let binding = FxRigBinding::new("track-1");
        assert!(!binding.is_bound());
        assert_eq!(binding.module_count(), 0);
    }

    #[test]
    fn bind_populates_fx_map() {
        let mut binding = FxRigBinding::new("track-1");
        let rig = DiscoveredRig {
            track_id: "track-1".into(),
            modules: vec![
                DiscoveredModule {
                    module_type: ModuleType::Drive,
                    fx_list: vec![make_fx("fx1", "Tube Screamer", 0)],
                    enabled: true,
                },
                DiscoveredModule {
                    module_type: ModuleType::Amp,
                    fx_list: vec![
                        make_fx("fx2", "Amp Sim", 1),
                        make_fx("fx3", "Cabinet IR", 2),
                    ],
                    enabled: true,
                },
            ],
        };

        binding.bind(rig);
        assert!(binding.is_bound());
        assert_eq!(binding.module_count(), 2);
        assert_eq!(binding.fx_ids_for_module(ModuleType::Drive).len(), 1);
        assert_eq!(binding.fx_ids_for_module(ModuleType::Amp).len(), 2);
    }

    #[test]
    fn unbind_clears_state() {
        let mut binding = FxRigBinding::new("track-1");
        binding.bind(DiscoveredRig {
            track_id: "track-1".into(),
            modules: vec![DiscoveredModule {
                module_type: ModuleType::Drive,
                fx_list: vec![make_fx("fx1", "OD", 0)],
                enabled: true,
            }],
        });
        assert!(binding.is_bound());

        binding.unbind();
        assert!(!binding.is_bound());
        assert_eq!(binding.module_count(), 0);
    }

    #[test]
    fn fx_ids_for_missing_module_returns_empty() {
        let binding = FxRigBinding::new("track-1");
        assert!(binding.fx_ids_for_module(ModuleType::Amp).is_empty());
    }
}
