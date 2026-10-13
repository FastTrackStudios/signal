//! Block definition registry for type-safe lookups.
//!
//! The `BlockRegistry` replaces ad-hoc string matching with a centralized,
//! type-safe lookup system for block definitions. It supports:
//!
//! - O(1) lookup by `type_id`
//! - Filtered listing by `BlockCategory`
//! - Fuzzy name matching for legacy code paths
//! - Thread-safe global access via `BlockRegistry::global()`

use std::collections::HashMap;
use std::sync::OnceLock;

use super::block::{BlockCategory, BlockDefinition, definitions};

/// Central registry of all known block definitions.
///
/// Block definitions are stored by `type_id` for O(1) lookup. The registry
/// is populated once at first access and is immutable thereafter.
pub struct BlockRegistry {
    by_type_id: HashMap<String, BlockDefinition>,
    /// Pre-computed list of (type_id, category) for fast category filtering.
    type_categories: Vec<(String, BlockCategory)>,
}

/// Static keywords for fuzzy name matching.
/// Each entry maps a set of name substrings to a `type_id`.
const NAME_HINTS: &[(&[&str], &str)] = &[
    (&["compressor", "comp"], "compressor"),
    (&["gate", "expander"], "gate"),
    (&["eq", "equalizer", "parametric"], "eq"),
    (
        &["drive", "od", "dist", "overdrive", "distortion", "fuzz"],
        "drive",
    ),
    (&["amp", "amplifier", "head"], "amp"),
    (&["cab", "cabinet", "ir", "speaker"], "cabinet"),
    (&["chorus"], "chorus"),
    (&["flanger", "flange"], "flanger"),
    (&["phaser", "phase"], "modulation"),
    (&["delay", "echo"], "delay"),
    (
        &["reverb", "verb", "room", "hall", "plate", "spring"],
        "reverb",
    ),
    (&["saturator", "saturation", "tape"], "saturator"),
    (&["deesser", "de-esser", "sibilance"], "deesser"),
    (&["tuner", "tune"], "tuner"),
    (&["input", "gain"], "input"),
    (&["volume", "vol", "output", "master"], "volume"),
];

impl BlockRegistry {
    /// Get the global block registry (lazily initialized).
    pub fn global() -> &'static Self {
        static INSTANCE: OnceLock<BlockRegistry> = OnceLock::new();
        INSTANCE.get_or_init(Self::new)
    }

    /// Create a new registry with all known block definitions.
    fn new() -> Self {
        let all_defs = vec![
            definitions::drive(),
            definitions::amp(),
            definitions::cabinet(),
            definitions::delay(),
            definitions::reverb(),
            definitions::compressor(),
            definitions::gate(),
            definitions::eq(),
            definitions::chorus(),
            definitions::flanger(),
            definitions::saturator(),
            definitions::deesser(),
            definitions::tuner(),
        ];

        let type_categories: Vec<(String, BlockCategory)> = all_defs
            .iter()
            .map(|d| (d.type_id.clone(), d.category))
            .collect();

        let by_type_id: HashMap<String, BlockDefinition> = all_defs
            .into_iter()
            .map(|d| (d.type_id.clone(), d))
            .collect();

        Self {
            by_type_id,
            type_categories,
        }
    }

    /// Look up a block definition by its exact `type_id`.
    #[must_use]
    pub fn get(&self, type_id: &str) -> Option<&BlockDefinition> {
        self.by_type_id.get(type_id)
    }

    /// Get all block definitions matching a category.
    #[must_use]
    pub fn get_by_category(&self, category: BlockCategory) -> Vec<&BlockDefinition> {
        self.type_categories
            .iter()
            .filter(|(_, cat)| *cat == category)
            .filter_map(|(id, _)| self.by_type_id.get(id))
            .collect()
    }

    /// Get all registered block definitions.
    #[must_use]
    pub fn all(&self) -> Vec<&BlockDefinition> {
        self.by_type_id.values().collect()
    }

    /// Get all registered type IDs.
    #[must_use]
    pub fn type_ids(&self) -> Vec<&str> {
        self.by_type_id.keys().map(String::as_str).collect()
    }

    /// Look up a block definition by fuzzy name matching.
    ///
    /// This replaces the ad-hoc `get_block_definition(name)` pattern used in
    /// UI code. It checks the name against known keywords and returns the best
    /// matching definition, or a default drive definition for unknown names.
    #[must_use]
    pub fn lookup_by_name(&self, name: &str) -> BlockDefinition {
        let name_lower = name.to_lowercase();

        // Try keyword matching
        for (keywords, type_id) in NAME_HINTS {
            for keyword in *keywords {
                if name_lower.contains(keyword)
                    && let Some(def) = self.by_type_id.get(*type_id)
                {
                    return def.clone();
                }
            }
        }

        // Fallback: return a drive definition with the given name
        let mut def = definitions::drive();
        def.name = name.to_string();
        def.type_id = name_lower.replace(' ', "_");
        def
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_all_definitions() {
        let reg = BlockRegistry::global();
        let all = reg.all();
        assert_eq!(all.len(), 13, "Expected 13 block definitions");
    }

    #[test]
    fn lookup_by_type_id() {
        let reg = BlockRegistry::global();

        assert!(reg.get("drive").is_some());
        assert!(reg.get("amp").is_some());
        assert!(reg.get("cabinet").is_some());
        assert!(reg.get("delay").is_some());
        assert!(reg.get("reverb").is_some());
        assert!(reg.get("compressor").is_some());
        assert!(reg.get("gate").is_some());
        assert!(reg.get("eq").is_some());
        assert!(reg.get("chorus").is_some());
        assert!(reg.get("flanger").is_some());
        assert!(reg.get("saturator").is_some());
        assert!(reg.get("deesser").is_some());
        assert!(reg.get("tuner").is_some());

        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn lookup_by_category() {
        let reg = BlockRegistry::global();

        let dynamics = reg.get_by_category(BlockCategory::Dynamics);
        assert_eq!(dynamics.len(), 2); // compressor + gate

        let drive = reg.get_by_category(BlockCategory::Drive);
        assert_eq!(drive.len(), 2); // drive + saturator

        let modulation = reg.get_by_category(BlockCategory::Modulation);
        assert_eq!(modulation.len(), 2); // chorus + flanger

        let delay = reg.get_by_category(BlockCategory::Delay);
        assert_eq!(delay.len(), 1);

        let reverb = reg.get_by_category(BlockCategory::Reverb);
        assert_eq!(reverb.len(), 1);

        let eq = reg.get_by_category(BlockCategory::Eq);
        assert_eq!(eq.len(), 2); // eq + deesser
    }

    #[test]
    fn fuzzy_name_lookup() {
        let reg = BlockRegistry::global();

        // Exact-ish matches
        assert_eq!(reg.lookup_by_name("Drive").type_id, "drive");
        assert_eq!(reg.lookup_by_name("Amp").type_id, "amp");
        assert_eq!(reg.lookup_by_name("Cabinet").type_id, "cabinet");
        assert_eq!(reg.lookup_by_name("Delay").type_id, "delay");
        assert_eq!(reg.lookup_by_name("Reverb").type_id, "reverb");
        assert_eq!(reg.lookup_by_name("Compressor").type_id, "compressor");
        assert_eq!(reg.lookup_by_name("Gate").type_id, "gate");

        // Partial/alias matches
        assert_eq!(reg.lookup_by_name("OD-1").type_id, "drive");
        assert_eq!(reg.lookup_by_name("Distortion Pedal").type_id, "drive");
        assert_eq!(reg.lookup_by_name("IR Loader").type_id, "cabinet");
        assert_eq!(reg.lookup_by_name("Echo Machine").type_id, "delay");
        assert_eq!(reg.lookup_by_name("Spring Reverb").type_id, "reverb");
        assert_eq!(reg.lookup_by_name("Room").type_id, "reverb");
        assert_eq!(reg.lookup_by_name("Hall Reverb").type_id, "reverb");
    }

    #[test]
    fn fuzzy_name_fallback() {
        let reg = BlockRegistry::global();

        let unknown = reg.lookup_by_name("Mystery Effect");
        assert_eq!(unknown.name, "Mystery Effect");
        assert_eq!(unknown.type_id, "mystery_effect");
    }

    #[test]
    fn type_ids_returns_all() {
        let reg = BlockRegistry::global();
        let ids = reg.type_ids();
        assert_eq!(ids.len(), 13);
        assert!(ids.contains(&"drive"));
        assert!(ids.contains(&"reverb"));
    }
}
