//! Screensets — high-level client wrapper around [`ScreensetService`].

use crate::DawClients;
use std::sync::Arc;

/// Handle for named FTS screenset management.
pub struct Screensets {
    clients: Arc<DawClients>,
}

impl Screensets {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn capture(
        &self,
        id: &str,
        name: &str,
        description: &str,
        kind: daw_proto::ScreensetKind,
        tags: Vec<String>,
        actions_on_apply: Vec<String>,
        options: daw_proto::ScreensetOptions,
    ) -> crate::Result<daw_proto::ScreensetResult> {
        Ok(self
            .clients
            .screenset
            .capture(daw_proto::CaptureScreensetRequest {
                id: id.to_string(),
                name: name.to_string(),
                description: description.to_string(),
                kind,
                tags,
                actions_on_apply,
                options,
            })
            .await?)
    }

    pub async fn save(
        &self,
        screenset: daw_proto::Screenset,
        options: daw_proto::ScreensetOptions,
    ) -> crate::Result<daw_proto::ScreensetResult> {
        Ok(self.clients.screenset.save(screenset, options).await?)
    }

    pub async fn list(
        &self,
        options: daw_proto::ScreensetOptions,
    ) -> crate::Result<Vec<daw_proto::ScreensetSummary>> {
        Ok(self.clients.screenset.list(options).await?)
    }

    pub async fn get(
        &self,
        id: &str,
        options: daw_proto::ScreensetOptions,
    ) -> crate::Result<Option<daw_proto::Screenset>> {
        Ok(self.clients.screenset.get(id.to_string(), options).await?)
    }

    pub async fn apply(
        &self,
        id: &str,
        options: daw_proto::ScreensetOptions,
    ) -> crate::Result<daw_proto::ScreensetResult> {
        Ok(self
            .clients
            .screenset
            .apply(id.to_string(), options)
            .await?)
    }

    pub async fn delete(
        &self,
        id: &str,
        options: daw_proto::ScreensetOptions,
    ) -> crate::Result<daw_proto::ScreensetResult> {
        Ok(self
            .clients
            .screenset
            .delete(id.to_string(), options)
            .await?)
    }
}
