use figma_model::document::FrameDocumentError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error(transparent)]
    Frame(#[from] FrameDocumentError),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}
