//! Scene template — reusable scene configurations that can be applied to any rig.
//!
//! A [`SceneTemplate`] captures engine-variant selections and parameter overrides
//! as a standalone stored entity, separate from any specific rig. This allows users
//! to build a library of scenes ("Clean", "Crunch", "Lead") and apply them across
//! different rigs and profiles.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::metadata::Metadata;
use crate::overrides::Override;
use crate::rig::EngineSelection;

// ─── IDs ────────────────────────────────────────────────────────

crate::typed_uuid_id!(
    /// Identifies a reusable scene template.
    SceneTemplateId
);

// ─── SceneTemplate ─────────────────────────────────────────────

/// A standalone, reusable scene configuration.
///
/// Unlike [`RigScene`](crate::rig::RigScene), which is embedded in a specific rig,
/// a `SceneTemplate` lives independently and can be applied to any compatible rig.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct SceneTemplate {
    pub id: SceneTemplateId,
    pub name: String,
    /// Which engine variant to use per engine slot.
    pub engine_selections: Vec<EngineSelection>,
    /// Parameter overrides applied when this scene is active.
    pub overrides: Vec<Override>,
    pub metadata: Metadata,
}

impl SceneTemplate {
    pub fn new(id: impl Into<SceneTemplateId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            engine_selections: Vec::new(),
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    #[must_use]
    pub fn with_engine(mut self, selection: EngineSelection) -> Self {
        self.engine_selections.push(selection);
        self
    }

    #[must_use]
    pub fn with_override(mut self, ov: Override) -> Self {
        self.overrides.push(ov);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Convert this template into a [`RigScene`](crate::rig::RigScene) with a new ID.
    pub fn to_rig_scene(&self, scene_id: impl Into<crate::rig::RigSceneId>) -> crate::rig::RigScene {
        crate::rig::RigScene {
            id: scene_id.into(),
            name: self.name.clone(),
            engine_selections: self.engine_selections.clone(),
            overrides: self.overrides.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

impl crate::traits::HasMetadata for SceneTemplate {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{EngineId, EngineSceneId};
    use crate::rig::EngineSelection;

    #[test]
    fn create_scene_template() {
        let template = SceneTemplate::new(SceneTemplateId::new(), "Clean")
            .with_engine(EngineSelection::new(
                EngineId::new(),
                EngineSceneId::new(),
            ));

        assert_eq!(template.name, "Clean");
        assert_eq!(template.engine_selections.len(), 1);
    }

    #[test]
    fn convert_to_rig_scene() {
        let template = SceneTemplate::new(SceneTemplateId::new(), "Lead")
            .with_engine(EngineSelection::new(
                EngineId::new(),
                EngineSceneId::new(),
            ));

        let scene = template.to_rig_scene(crate::rig::RigSceneId::new());
        assert_eq!(scene.name, "Lead");
        assert_eq!(scene.engine_selections.len(), 1);
    }
}
