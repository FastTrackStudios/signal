//! Dynamic panel registry — runtime-extensible panel registration.
//!
//! Any crate can contribute panels by registering a [`PanelDescriptor`] at
//! runtime. The [`PanelRegistry`] replaces the need for a hardcoded enum as
//! the single source of truth for what panels exist.
//!
//! The existing [`PanelId`](crate::panel::PanelId) enum continues to work
//! via its `Into<String>` impl for backward compatibility.

use facet::Facet;
use std::collections::HashMap;

use crate::context::{ActionContext, WhenExpr};

// region: --- Types

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum DockPosition {
    Left,
    Right,
    Bottom,
    Center,
    Float,
}

/// Size constraints for a panel, used by the layout engine during resize
/// negotiation and constraint solving.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct PanelConstraints {
    pub min_width: Option<f64>,
    pub max_width: Option<f64>,
    pub min_height: Option<f64>,
    pub max_height: Option<f64>,
    pub preferred_width: Option<f64>,
    pub preferred_height: Option<f64>,
    /// Higher priority panels get space first during layout negotiation (0–255).
    pub resize_priority: u8,
}

impl Default for PanelConstraints {
    fn default() -> Self {
        Self {
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            preferred_width: None,
            preferred_height: None,
            resize_priority: 128,
        }
    }
}

impl PanelConstraints {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_min_width(mut self, width: f64) -> Self {
        self.min_width = Some(width);
        self
    }

    pub fn with_max_width(mut self, width: f64) -> Self {
        self.max_width = Some(width);
        self
    }

    pub fn with_min_height(mut self, height: f64) -> Self {
        self.min_height = Some(height);
        self
    }

    pub fn with_max_height(mut self, height: f64) -> Self {
        self.max_height = Some(height);
        self
    }

    pub fn with_preferred_width(mut self, width: f64) -> Self {
        self.preferred_width = Some(width);
        self
    }

    pub fn with_preferred_height(mut self, height: f64) -> Self {
        self.preferred_height = Some(height);
        self
    }

    pub fn with_resize_priority(mut self, priority: u8) -> Self {
        self.resize_priority = priority;
        self
    }
}

/// Describes a panel that can be registered with the dock system.
///
/// This is the runtime metadata for a panel. The dock system uses it for
/// display names, icons, default placement, and constraint negotiation.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct PanelDescriptor {
    /// Unique string identifier for this panel.
    pub id: String,
    /// Human-readable display name shown in tabs and headers.
    pub display_name: String,
    /// Icon identifier (e.g. lucide icon name).
    pub icon: String,
    /// Category for grouping in panel pickers (e.g. "Session", "Signal", "Utility").
    pub category: String,
    /// Preferred default position when auto-placing this panel.
    pub default_position: DockPosition,
    /// Size constraints for layout negotiation.
    pub constraints: PanelConstraints,
    /// Optional when-clause controlling contextual visibility.
    pub visibility_when: Option<String>,
}

impl PanelDescriptor {
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            icon: String::new(),
            category: String::new(),
            default_position: DockPosition::Center,
            constraints: PanelConstraints::default(),
            visibility_when: None,
        }
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    pub fn with_default_position(mut self, position: DockPosition) -> Self {
        self.default_position = position;
        self
    }

    pub fn with_constraints(mut self, constraints: PanelConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    pub fn with_visibility_when(mut self, when: impl Into<String>) -> Self {
        self.visibility_when = Some(when.into());
        self
    }
}

// endregion: --- Types

// region: --- PanelRegistry

/// Runtime registry of available panels.
///
/// This is the single source of truth for what panels exist in the application.
/// Any crate can register panels at startup via [`register`](Self::register).
#[derive(Debug, Clone, Default)]
pub struct PanelRegistry {
    panels: HashMap<String, PanelDescriptor>,
}

impl PanelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a panel descriptor. Overwrites any existing panel with the same ID.
    pub fn register(&mut self, descriptor: PanelDescriptor) {
        self.panels.insert(descriptor.id.clone(), descriptor);
    }

    /// Remove a panel by ID. Returns the removed descriptor if it existed.
    pub fn unregister(&mut self, id: &str) -> Option<PanelDescriptor> {
        self.panels.remove(id)
    }

    /// Look up a panel descriptor by ID.
    pub fn get(&self, id: &str) -> Option<&PanelDescriptor> {
        self.panels.get(id)
    }

    /// Get all registered panel descriptors.
    pub fn all(&self) -> Vec<&PanelDescriptor> {
        self.panels.values().collect()
    }

    /// Get all registered panel IDs.
    pub fn all_ids(&self) -> Vec<&str> {
        self.panels.keys().map(|s| s.as_str()).collect()
    }

    /// Get panels grouped by category.
    ///
    /// Returns a map from category name to the descriptors in that category.
    /// Panels with an empty category are grouped under `""`.
    pub fn by_category(&self) -> HashMap<&str, Vec<&PanelDescriptor>> {
        let mut groups: HashMap<&str, Vec<&PanelDescriptor>> = HashMap::new();
        for desc in self.panels.values() {
            groups.entry(desc.category.as_str()).or_default().push(desc);
        }
        groups
    }

    /// Number of registered panels.
    pub fn len(&self) -> usize {
        self.panels.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.panels.is_empty()
    }

    /// Check if a panel ID is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.panels.contains_key(id)
    }

    /// Return IDs of panels visible in the given action context.
    pub fn visible_panels(&self, ctx: &ActionContext) -> Vec<String> {
        self.panels
            .values()
            .filter(|desc| {
                desc.visibility_when
                    .as_deref()
                    .map(|expr| WhenExpr::parse(expr).evaluate(ctx))
                    .unwrap_or(true)
            })
            .map(|desc| desc.id.clone())
            .collect()
    }

    /// Return IDs of panels that have context visibility rules.
    pub fn context_managed_panels(&self) -> Vec<String> {
        self.panels
            .values()
            .filter(|desc| desc.visibility_when.is_some())
            .map(|desc| desc.id.clone())
            .collect()
    }
}

// endregion: --- PanelRegistry

#[cfg(test)]
mod tests {
    use super::*;

    // -- Setup & Fixtures

    fn sample_descriptor(id: &str, category: &str) -> PanelDescriptor {
        PanelDescriptor::new(id, id)
            .with_icon("test-icon")
            .with_category(category)
            .with_default_position(DockPosition::Center)
    }

    fn populated_registry() -> PanelRegistry {
        let mut reg = PanelRegistry::new();
        reg.register(sample_descriptor("performance", "Session"));
        reg.register(sample_descriptor("chart-editor", "Session"));
        reg.register(sample_descriptor("navigator", "Utility"));
        reg.register(sample_descriptor("inspector", "Utility"));
        reg.register(sample_descriptor("rig-grid", "Signal"));
        reg
    }

    // -- Tests

    #[test]
    fn test_registry_register_and_get() {
        // -- Setup & Fixtures
        let mut reg = PanelRegistry::new();
        let desc = sample_descriptor("my-panel", "Custom");

        // -- Exec
        reg.register(desc.clone());

        // -- Check
        let retrieved = reg.get("my-panel").unwrap();
        assert_eq!(retrieved.id, "my-panel");
        assert_eq!(retrieved.display_name, "my-panel");
        assert_eq!(retrieved.category, "Custom");
        assert_eq!(retrieved.icon, "test-icon");
    }

    #[test]
    fn test_registry_unregister() {
        // -- Setup & Fixtures
        let mut reg = populated_registry();
        assert!(reg.contains("navigator"));

        // -- Exec
        let removed = reg.unregister("navigator");

        // -- Check
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "navigator");
        assert!(!reg.contains("navigator"));
        assert!(reg.get("navigator").is_none());
        assert_eq!(reg.len(), 4);
    }

    #[test]
    fn test_registry_unregister_nonexistent() {
        // -- Setup & Fixtures
        let mut reg = PanelRegistry::new();

        // -- Exec
        let removed = reg.unregister("does-not-exist");

        // -- Check
        assert!(removed.is_none());
    }

    #[test]
    fn test_registry_all() {
        // -- Setup & Fixtures
        let reg = populated_registry();

        // -- Exec
        let all = reg.all();

        // -- Check
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn test_registry_by_category() {
        // -- Setup & Fixtures
        let reg = populated_registry();

        // -- Exec
        let groups = reg.by_category();

        // -- Check
        assert_eq!(groups.len(), 3); // Session, Utility, Signal
        assert_eq!(groups["Session"].len(), 2);
        assert_eq!(groups["Utility"].len(), 2);
        assert_eq!(groups["Signal"].len(), 1);
    }

    #[test]
    fn test_registry_overwrite_on_reregister() {
        // -- Setup & Fixtures
        let mut reg = PanelRegistry::new();
        reg.register(sample_descriptor("my-panel", "Old"));

        // -- Exec
        reg.register(PanelDescriptor::new("my-panel", "Updated Name").with_category("New"));

        // -- Check
        let desc = reg.get("my-panel").unwrap();
        assert_eq!(desc.display_name, "Updated Name");
        assert_eq!(desc.category, "New");
        assert_eq!(reg.len(), 1); // still just one entry
    }

    #[test]
    fn test_registry_empty() {
        // -- Setup & Fixtures
        let reg = PanelRegistry::new();

        // -- Check
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(reg.all().is_empty());
        assert!(reg.by_category().is_empty());
    }

    #[test]
    fn test_visible_panels_respects_when() {
        let mut reg = PanelRegistry::new();
        reg.register(
            PanelDescriptor::new("transport", "Transport").with_visibility_when("tab:performance"),
        );
        reg.register(PanelDescriptor::new("settings", "Settings"));

        let mut ctx = ActionContext::new();
        ctx.set_tab("chart");
        let visible = reg.visible_panels(&ctx);
        assert!(visible.contains(&"settings".to_string()));
        assert!(!visible.contains(&"transport".to_string()));

        ctx.set_tab("performance");
        let visible = reg.visible_panels(&ctx);
        assert!(visible.contains(&"transport".to_string()));
    }

    #[test]
    fn test_panel_constraints_builder() {
        // -- Exec
        let constraints = PanelConstraints::new()
            .with_min_width(200.0)
            .with_max_width(500.0)
            .with_min_height(100.0)
            .with_preferred_width(300.0)
            .with_resize_priority(200);

        // -- Check
        assert_eq!(constraints.min_width, Some(200.0));
        assert_eq!(constraints.max_width, Some(500.0));
        assert_eq!(constraints.min_height, Some(100.0));
        assert_eq!(constraints.max_height, None);
        assert_eq!(constraints.preferred_width, Some(300.0));
        assert_eq!(constraints.preferred_height, None);
        assert_eq!(constraints.resize_priority, 200);
    }

    #[test]
    fn test_panel_descriptor_builder() {
        // -- Exec
        let desc = PanelDescriptor::new("my-panel", "My Panel")
            .with_icon("star")
            .with_category("Custom")
            .with_default_position(DockPosition::Left)
            .with_constraints(PanelConstraints::new().with_min_width(150.0));

        // -- Check
        assert_eq!(desc.id, "my-panel");
        assert_eq!(desc.display_name, "My Panel");
        assert_eq!(desc.icon, "star");
        assert_eq!(desc.category, "Custom");
        assert_eq!(desc.default_position, DockPosition::Left);
        assert_eq!(desc.constraints.min_width, Some(150.0));
    }

    #[test]
    fn test_dock_position_variants() {
        // -- Check
        let positions = [
            DockPosition::Left,
            DockPosition::Right,
            DockPosition::Bottom,
            DockPosition::Center,
            DockPosition::Float,
        ];
        // All variants are distinct
        for (i, a) in positions.iter().enumerate() {
            for (j, b) in positions.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn test_panel_id_into_string() {
        use crate::panel::PanelId;

        // -- Exec
        let key: String = PanelId::Performance.into();

        // -- Check
        assert_eq!(key, "performance");
        assert_eq!(PanelId::ChartEditor.as_str(), "chart-editor");
        assert_eq!(PanelId::Navigator.as_str(), "navigator");
    }

    #[test]
    fn test_panel_id_roundtrip_str() {
        use crate::panel::PanelId;

        // -- Check: every variant survives as_str -> from_str_id roundtrip
        for &panel in PanelId::all() {
            let s = panel.as_str();
            let restored = PanelId::from_str_id(s);
            assert_eq!(restored, Some(panel), "roundtrip failed for {s}");
        }

        // Unknown string returns None
        assert_eq!(PanelId::from_str_id("custom-plugin-panel"), None);
    }

    #[test]
    fn test_panel_id_register_all() {
        use crate::panel::PanelId;

        // -- Setup & Fixtures
        let mut reg = PanelRegistry::new();

        // -- Exec
        PanelId::register_all(&mut reg);

        // -- Check
        assert_eq!(reg.len(), PanelId::all().len());

        // Every PanelId variant is in the registry under its string key
        for &panel in PanelId::all() {
            let desc = reg.get(panel.as_str()).unwrap();
            assert_eq!(desc.display_name, panel.display_name());
            assert_eq!(desc.icon, panel.icon_name());
        }

        // Categories are populated correctly
        let groups = reg.by_category();
        assert!(groups.contains_key("Session"));
        assert!(groups.contains_key("Signal"));
        assert!(groups.contains_key("DAW"));
        assert!(groups.contains_key("Utility"));
    }

    #[test]
    fn test_panel_id_coexists_with_dynamic_panels() {
        use crate::panel::PanelId;

        // -- Setup & Fixtures
        let mut reg = PanelRegistry::new();
        PanelId::register_all(&mut reg);
        let initial_count = reg.len();

        // -- Exec: register a dynamic panel alongside built-in ones
        reg.register(
            PanelDescriptor::new("my-custom-plugin", "Custom Plugin")
                .with_icon("plug")
                .with_category("Plugins")
                .with_default_position(DockPosition::Right),
        );

        // -- Check
        assert_eq!(reg.len(), initial_count + 1);
        assert!(reg.contains("my-custom-plugin"));
        assert!(reg.contains(PanelId::Performance.as_str()));

        // Built-in and dynamic panels coexist
        let groups = reg.by_category();
        assert_eq!(groups["Plugins"].len(), 1);
    }
}
