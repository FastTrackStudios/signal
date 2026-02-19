use crate::{SignalApi, SignalController};
use signal_proto::{
    layer::{Layer, LayerId, LayerSnapshot, LayerSnapshotId},
    EngineType,
};

/// Handle for layer operations.
pub struct LayerOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> LayerOps<S> {
    pub async fn list(&self) -> Vec<Layer> {
        let cx = self.0.context_factory.make_context();
        self.0.service.list_layers(&cx).await
    }

    pub async fn load(&self, id: impl Into<LayerId>) -> Option<Layer> {
        let cx = self.0.context_factory.make_context();
        self.0.service.load_layer(&cx, id.into()).await
    }

    pub async fn create(&self, name: impl Into<String>, engine_type: EngineType) -> Layer {
        let layer = Layer::new(
            LayerId::new(),
            name,
            engine_type,
            LayerSnapshot::new(LayerSnapshotId::new(), "Default"),
        );
        self.save(layer.clone()).await;
        layer
    }

    pub async fn save(&self, layer: Layer) -> Layer {
        let cx = self.0.context_factory.make_context();
        self.0.service.save_layer(&cx, layer.clone()).await;
        layer
    }

    pub async fn delete(&self, id: impl Into<LayerId>) {
        let cx = self.0.context_factory.make_context();
        self.0.service.delete_layer(&cx, id.into()).await;
    }

    pub async fn load_variant(
        &self,
        layer_id: impl Into<LayerId>,
        variant_id: impl Into<LayerSnapshotId>,
    ) -> Option<LayerSnapshot> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .load_layer_variant(&cx, layer_id.into(), variant_id.into())
            .await
    }

    pub async fn save_variant(&self, layer_id: impl Into<LayerId>, snapshot: LayerSnapshot) {
        let layer_id = layer_id.into();
        if let Some(mut layer) = self.load(layer_id).await {
            if let Some(pos) = layer.variants.iter().position(|v| v.id == snapshot.id) {
                layer.variants[pos] = snapshot;
            } else {
                layer.variants.push(snapshot);
            }
            self.save(layer).await;
        }
    }

    pub async fn by_tag(&self, tag: &str) -> Vec<Layer> {
        let all = self.list().await;
        all.into_iter()
            .filter(|l| l.metadata.tags.contains(tag))
            .collect()
    }

    pub async fn find_by_name(&self, name: &str) -> Option<Layer> {
        self.list().await.into_iter().find(|l| l.name == name)
    }

    pub async fn rename(&self, id: impl Into<LayerId>, new_name: impl Into<String>) {
        if let Some(mut layer) = self.load(id).await {
            layer.name = new_name.into();
            self.save(layer).await;
        }
    }

    /// Load a layer, apply a closure to one of its snapshots, and save.
    pub async fn update_variant(
        &self,
        layer_id: impl Into<LayerId>,
        variant_id: impl Into<LayerSnapshotId>,
        f: impl FnOnce(&mut LayerSnapshot),
    ) {
        let layer_id = layer_id.into();
        let variant_id = variant_id.into();
        if let Some(mut layer) = self.load(layer_id).await {
            if let Some(v) = layer.variants.iter_mut().find(|v| v.id == variant_id) {
                f(v);
            }
            self.save(layer).await;
        }
    }
}
