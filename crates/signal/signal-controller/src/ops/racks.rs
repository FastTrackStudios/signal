use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::rack::{Rack, RackId};

/// Handle for rack operations.
pub struct RackOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> RackOps<S> {
    pub async fn list(&self) -> Result<Vec<Rack>, OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .list_racks(&cx)
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load(&self, id: impl Into<RackId>) -> Result<Option<Rack>, OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .load_rack(&cx, id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn save(&self, rack: Rack) -> Result<Rack, OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .save_rack(&cx, rack.clone())
            .await
            .map_err(OpsError::Storage)?;
        Ok(rack)
    }

    pub async fn delete(&self, id: impl Into<RackId>) -> Result<(), OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .delete_rack(&cx, id.into())
            .await
            .map_err(OpsError::Storage)
    }
}
