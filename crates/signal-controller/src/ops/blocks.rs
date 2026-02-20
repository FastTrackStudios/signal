use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::{Block, BlockType};

/// Handle for raw block parameter operations.
pub struct BlockOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> BlockOps<S> {
    pub async fn get(&self, block_type: BlockType) -> Result<Block, OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .get_block(&cx, block_type)
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn set(&self, block_type: BlockType, block: Block) -> Result<Block, OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .set_block(&cx, block_type, block)
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn get_value(&self, block_type: BlockType) -> Result<f32, OpsError> {
        Ok(self.get(block_type).await?.first_value().unwrap_or(0.0))
    }

    pub async fn set_value(&self, block_type: BlockType, value: f32) -> Result<Block, OpsError> {
        let mut block = self.get(block_type).await?;
        block.set_first_value(value);
        self.set(block_type, block).await
    }
}
