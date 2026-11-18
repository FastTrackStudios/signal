//! Modulation routing — connects sources to parameter targets.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::sources::ModulationSource;
use crate::target::ParamTarget;

/// A single modulation route connecting a source to a target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ModulationRoute {
    /// Unique ID for this route.
    pub id: String,
    /// The modulation source.
    pub source: ModulationSource,
    /// The parameter being modulated.
    pub target: ParamTarget,
    /// Modulation amount (-1.0 to 1.0, negative = inverted).
    pub amount: f32,
    /// Whether this route is active.
    pub enabled: bool,
}

impl ModulationRoute {
    pub fn new(
        id: impl Into<String>,
        source: ModulationSource,
        target: ParamTarget,
        amount: f32,
    ) -> Self {
        Self {
            id: id.into(),
            source,
            target,
            amount: amount.clamp(-1.0, 1.0),
            enabled: true,
        }
    }
}

/// Collection of modulation routes for a rig/scene.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Facet)]
pub struct ModulationRouteSet {
    pub routes: Vec<ModulationRoute>,
}

impl ModulationRouteSet {
    #[must_use]
    pub const fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn add(&mut self, route: ModulationRoute) {
        self.routes.push(route);
    }

    pub fn remove(&mut self, id: &str) {
        self.routes.retain(|r| r.id != id);
    }

    /// All active routes targeting a specific parameter.
    #[must_use]
    pub fn routes_for_param(&self, block_id: &str, param_id: &str) -> Vec<&ModulationRoute> {
        self.routes
            .iter()
            .filter(|r| r.enabled && r.target.block_id == block_id && r.target.param_id == param_id)
            .collect()
    }
}
