use crate::{SignalApi, SignalController};
use signal_proto::rack::{Rack, RackId};

/// Handle for rack operations.
pub struct RackOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> RackOps<S> {
    pub async fn list(&self) -> Vec<Rack> {
        let cx = self.0.context_factory.make_context();
        self.0.service.list_racks(&cx).await
    }

    pub async fn load(&self, id: impl Into<RackId>) -> Option<Rack> {
        let cx = self.0.context_factory.make_context();
        self.0.service.load_rack(&cx, id.into()).await
    }

    pub async fn save(&self, rack: Rack) -> Rack {
        let cx = self.0.context_factory.make_context();
        self.0.service.save_rack(&cx, rack.clone()).await;
        rack
    }

    pub async fn delete(&self, id: impl Into<RackId>) {
        let cx = self.0.context_factory.make_context();
        self.0.service.delete_rack(&cx, id.into()).await;
    }
}
