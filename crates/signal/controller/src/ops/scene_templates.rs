//! Scene template operations — CRUD for reusable scene configurations.
//!
//! Provides [`SceneTemplateOps`], a controller handle for managing standalone
//! scene templates that can be applied across different rigs.

use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::scene_template::{SceneTemplate, SceneTemplateId};

/// Handle for scene template operations.
pub struct SceneTemplateOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> SceneTemplateOps<S> {
    /// List all scene templates.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage layer fails.
    pub async fn list(&self) -> Result<Vec<SceneTemplate>, OpsError> {
        self.0
            .service
            .list_scene_templates()
            .await
            .map_err(OpsError::Storage)
    }

    /// Load a scene template by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage layer fails.
    pub async fn load(
        &self,
        id: impl Into<SceneTemplateId>,
    ) -> Result<Option<SceneTemplate>, OpsError> {
        self.0
            .service
            .load_scene_template(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    /// Save a scene template.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage layer fails.
    pub async fn save(&self, template: SceneTemplate) -> Result<SceneTemplate, OpsError> {
        self.0
            .service
            .save_scene_template(template.clone())
            .await
            .map_err(OpsError::Storage)?;
        Ok(template)
    }

    /// Delete a scene template.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage layer fails.
    pub async fn delete(&self, id: impl Into<SceneTemplateId>) -> Result<(), OpsError> {
        self.0
            .service
            .delete_scene_template(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    /// Reorder scene templates.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage layer fails.
    pub async fn reorder(&self, ordered_ids: Vec<SceneTemplateId>) -> Result<(), OpsError> {
        self.0
            .service
            .reorder_scene_templates(ordered_ids)
            .await
            .map_err(OpsError::Storage)
    }
}
