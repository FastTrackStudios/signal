//! Rack operations — CRUD for racks and rack slots.
//!
//! Provides [`RackOps`], a controller handle for managing racks,
//! the top-level grouping above rigs in the signal hierarchy.

use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::rack::{Rack, RackId};

/// Handle for rack operations.
pub struct RackOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> RackOps<S> {
    pub async fn list(&self) -> Result<Vec<Rack>, OpsError> {
        self.0.service.list_racks().await.map_err(OpsError::Storage)
    }

    pub async fn load(&self, id: impl Into<RackId>) -> Result<Option<Rack>, OpsError> {
        self.0
            .service
            .load_rack(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn save(&self, rack: Rack) -> Result<Rack, OpsError> {
        self.0
            .service
            .save_rack(rack.clone())
            .await
            .map_err(OpsError::Storage)?;
        Ok(rack)
    }

    pub async fn delete(&self, id: impl Into<RackId>) -> Result<(), OpsError> {
        self.0
            .service
            .delete_rack(id.into())
            .await
            .map_err(OpsError::Storage)
    }
}
