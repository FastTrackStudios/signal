use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::{
    layer::{Layer, LayerId, LayerSnapshot, LayerSnapshotId},
    EngineType,
};

/// Handle for layer operations.
pub struct LayerOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> LayerOps<S> {
    pub async fn list(&self) -> Result<Vec<Layer>, OpsError> {
        self.0
            .service
            .list_layers()
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load(&self, id: impl Into<LayerId>) -> Result<Option<Layer>, OpsError> {
        self.0
            .service
            .load_layer(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn create(
        &self,
        name: impl Into<String>,
        engine_type: EngineType,
    ) -> Result<Layer, OpsError> {
        let layer = Layer::new(
            LayerId::new(),
            name,
            engine_type,
            LayerSnapshot::new(LayerSnapshotId::new(), "Default"),
        );
        self.save(layer.clone()).await?;
        Ok(layer)
    }

    pub async fn save(&self, layer: Layer) -> Result<Layer, OpsError> {
        self.0
            .service
            .save_layer(layer.clone())
            .await
            .map_err(OpsError::Storage)?;
        Ok(layer)
    }

    pub async fn delete(&self, id: impl Into<LayerId>) -> Result<(), OpsError> {
        self.0
            .service
            .delete_layer(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load_variant(
        &self,
        layer_id: impl Into<LayerId>,
        variant_id: impl Into<LayerSnapshotId>,
    ) -> Result<Option<LayerSnapshot>, OpsError> {
        self.0
            .service
            .load_layer_variant(layer_id.into(), variant_id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn save_variant(
        &self,
        layer_id: impl Into<LayerId>,
        snapshot: LayerSnapshot,
    ) -> Result<(), OpsError> {
        let layer_id = layer_id.into();
        if let Some(mut layer) = self.load(layer_id).await? {
            if let Some(pos) = layer.variants.iter().position(|v| v.id == snapshot.id) {
                layer.variants[pos] = snapshot;
            } else {
                layer.variants.push(snapshot);
            }
            self.save(layer).await?;
        }
        Ok(())
    }

    pub async fn by_tag(&self, tag: &str) -> Result<Vec<Layer>, OpsError> {
        let all = self.list().await?;
        Ok(all
            .into_iter()
            .filter(|l| l.metadata.tags.contains(tag))
            .collect())
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Layer>, OpsError> {
        Ok(self.list().await?.into_iter().find(|l| l.name == name))
    }

    pub async fn rename(
        &self,
        id: impl Into<LayerId>,
        new_name: impl Into<String>,
    ) -> Result<(), OpsError> {
        if let Some(mut layer) = self.load(id).await? {
            layer.name = new_name.into();
            self.save(layer).await?;
        }
        Ok(())
    }

    /// Load a layer, apply a closure to one of its snapshots, and save.
    pub async fn update_variant(
        &self,
        layer_id: impl Into<LayerId>,
        variant_id: impl Into<LayerSnapshotId>,
        f: impl FnOnce(&mut LayerSnapshot),
    ) -> Result<(), OpsError> {
        let layer_id = layer_id.into();
        let variant_id = variant_id.into();
        if let Some(mut layer) = self.load(layer_id).await? {
            if let Some(v) = layer.variants.iter_mut().find(|v| v.id == variant_id) {
                f(v);
            }
            self.save(layer).await?;
        }
        Ok(())
    }

    /// Add a variant to a layer. Returns the updated layer, or `None` if the layer doesn't exist.
    pub async fn add_variant(
        &self,
        layer_id: impl Into<LayerId>,
        snapshot: LayerSnapshot,
    ) -> Result<Option<Layer>, OpsError> {
        let layer_id = layer_id.into();
        if let Some(mut layer) = self.load(layer_id).await? {
            layer.add_variant(snapshot);
            Ok(Some(self.save(layer).await?))
        } else {
            Ok(None)
        }
    }

    /// Remove a variant from a layer. Returns the removed snapshot, or `None` if not found.
    pub async fn remove_variant(
        &self,
        layer_id: impl Into<LayerId>,
        snapshot_id: impl Into<LayerSnapshotId>,
    ) -> Result<Option<LayerSnapshot>, OpsError> {
        let layer_id = layer_id.into();
        let snapshot_id = snapshot_id.into();
        if let Some(mut layer) = self.load(layer_id).await? {
            let removed = layer.remove_variant(&snapshot_id);
            if removed.is_some() {
                self.save(layer).await?;
            }
            Ok(removed)
        } else {
            Ok(None)
        }
    }

    /// Duplicate a variant within a layer. Returns the new snapshot, or `None` if not found.
    pub async fn duplicate_variant(
        &self,
        layer_id: impl Into<LayerId>,
        snapshot_id: impl Into<LayerSnapshotId>,
        new_name: impl Into<String>,
    ) -> Result<Option<LayerSnapshot>, OpsError> {
        let layer_id = layer_id.into();
        let snapshot_id = snapshot_id.into();
        if let Some(mut layer) = self.load(layer_id).await? {
            if let Some(original) = layer.variant(&snapshot_id) {
                let dup = original.duplicate(LayerSnapshotId::new(), new_name);
                let dup_clone = dup.clone();
                layer.add_variant(dup);
                self.save(layer).await?;
                Ok(Some(dup_clone))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Check if a layer exists.
    pub async fn exists(&self, id: impl Into<LayerId>) -> Result<bool, OpsError> {
        Ok(self.load(id).await?.is_some())
    }

    /// Count all layers.
    pub async fn count(&self) -> Result<usize, OpsError> {
        Ok(self.list().await?.len())
    }

    // region: --- try_* variants

    /// Add a variant, returning an error if the layer doesn't exist.
    pub async fn try_add_variant(
        &self,
        layer_id: impl Into<LayerId>,
        snapshot: LayerSnapshot,
    ) -> Result<Layer, OpsError> {
        let layer_id = layer_id.into();
        let mut layer = self
            .load(layer_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "layer",
                id: layer_id.to_string(),
            })?;
        layer.add_variant(snapshot);
        Ok(self.save(layer).await?)
    }

    /// Remove a variant, returning an error if the layer or variant doesn't exist.
    pub async fn try_remove_variant(
        &self,
        layer_id: impl Into<LayerId>,
        snapshot_id: impl Into<LayerSnapshotId>,
    ) -> Result<LayerSnapshot, OpsError> {
        let layer_id = layer_id.into();
        let snapshot_id = snapshot_id.into();
        let mut layer = self
            .load(layer_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "layer",
                id: layer_id.to_string(),
            })?;
        let removed =
            layer
                .remove_variant(&snapshot_id)
                .ok_or_else(|| OpsError::VariantNotFound {
                    entity_type: "variant",
                    parent_id: layer_id.to_string(),
                    variant_id: snapshot_id.to_string(),
                })?;
        self.save(layer).await?;
        Ok(removed)
    }

    /// Duplicate a variant, returning an error if the layer or variant doesn't exist.
    pub async fn try_duplicate_variant(
        &self,
        layer_id: impl Into<LayerId>,
        snapshot_id: impl Into<LayerSnapshotId>,
        new_name: impl Into<String>,
    ) -> Result<LayerSnapshot, OpsError> {
        let layer_id = layer_id.into();
        let snapshot_id = snapshot_id.into();
        let mut layer = self
            .load(layer_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "layer",
                id: layer_id.to_string(),
            })?;
        let original = layer
            .variant(&snapshot_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "variant",
                parent_id: layer_id.to_string(),
                variant_id: snapshot_id.to_string(),
            })?;
        let dup = original.duplicate(LayerSnapshotId::new(), new_name);
        let dup_clone = dup.clone();
        layer.add_variant(dup);
        self.save(layer).await?;
        Ok(dup_clone)
    }

    /// Save a variant within a layer, returning an error if the layer doesn't exist.
    pub async fn try_save_variant(
        &self,
        layer_id: impl Into<LayerId>,
        snapshot: LayerSnapshot,
    ) -> Result<(), OpsError> {
        let layer_id = layer_id.into();
        let mut layer = self
            .load(layer_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "layer",
                id: layer_id.to_string(),
            })?;
        if let Some(pos) = layer.variants.iter().position(|v| v.id == snapshot.id) {
            layer.variants[pos] = snapshot;
        } else {
            layer.variants.push(snapshot);
        }
        self.save(layer).await?;
        Ok(())
    }

    /// Update a variant via closure, returning an error if the layer or variant doesn't exist.
    pub async fn try_update_variant(
        &self,
        layer_id: impl Into<LayerId>,
        variant_id: impl Into<LayerSnapshotId>,
        f: impl FnOnce(&mut LayerSnapshot),
    ) -> Result<(), OpsError> {
        let layer_id = layer_id.into();
        let variant_id = variant_id.into();
        let mut layer = self
            .load(layer_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "layer",
                id: layer_id.to_string(),
            })?;
        let variant = layer
            .variants
            .iter_mut()
            .find(|v| v.id == variant_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "variant",
                parent_id: layer_id.to_string(),
                variant_id: variant_id.to_string(),
            })?;
        f(variant);
        self.save(layer).await?;
        Ok(())
    }

    // endregion: --- try_* variants
}
