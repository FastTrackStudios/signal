//! TempoMap handle and operations

use std::sync::Arc;

use crate::DawClients;
use crate::Result;
use daw_proto::{ProjectContext, TempoPoint};

#[derive(Clone)]
pub struct TempoMap {
    project_id: String,
    clients: Arc<DawClients>,
}

impl TempoMap {
    pub(crate) fn new(project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            project_id,
            clients,
        }
    }

    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    pub async fn points(&self) -> Result<Vec<TempoPoint>> {
        Ok(self
            .clients
            .tempo_map
            .get_tempo_points(self.context())
            .await?)
    }

    pub async fn point(&self, index: u32) -> Result<Option<TempoPoint>> {
        Ok(self
            .clients
            .tempo_map
            .get_tempo_point(self.context(), index)
            .await?)
    }

    pub async fn count(&self) -> Result<usize> {
        Ok(self
            .clients
            .tempo_map
            .tempo_point_count(self.context())
            .await? as usize)
    }

    pub async fn tempo_at(&self, seconds: f64) -> Result<f64> {
        Ok(self
            .clients
            .tempo_map
            .get_tempo_at(self.context(), seconds)
            .await?)
    }

    pub async fn time_signature_at(&self, seconds: f64) -> Result<(i32, i32)> {
        Ok(self
            .clients
            .tempo_map
            .get_time_signature_at(self.context(), seconds)
            .await?)
    }

    pub async fn time_to_musical(&self, seconds: f64) -> Result<(i32, i32, f64)> {
        Ok(self
            .clients
            .tempo_map
            .time_to_musical(self.context(), seconds)
            .await?)
    }

    pub async fn musical_to_time(&self, measure: i32, beat: i32, fraction: f64) -> Result<f64> {
        Ok(self
            .clients
            .tempo_map
            .musical_to_time(self.context(), measure, beat, fraction)
            .await?)
    }

    pub async fn add_point(&self, seconds: f64, bpm: f64) -> Result<u32> {
        Ok(self
            .clients
            .tempo_map
            .add_tempo_point(self.context(), seconds, bpm)
            .await??)
    }

    pub async fn remove_point(&self, index: u32) -> Result<()> {
        self.clients
            .tempo_map
            .remove_tempo_point(self.context(), index)
            .await??;
        Ok(())
    }

    pub async fn set_tempo_at(&self, index: u32, bpm: f64) -> Result<()> {
        self.clients
            .tempo_map
            .set_tempo_at_point(self.context(), index, bpm)
            .await??;
        Ok(())
    }

    pub async fn set_time_signature_at(
        &self,
        index: u32,
        numerator: i32,
        denominator: i32,
    ) -> Result<()> {
        self.clients
            .tempo_map
            .set_time_signature_at_point(self.context(), index, numerator, denominator)
            .await??;
        Ok(())
    }

    pub async fn move_point(&self, index: u32, seconds: f64) -> Result<()> {
        self.clients
            .tempo_map
            .move_tempo_point(self.context(), index, seconds)
            .await??;
        Ok(())
    }

    pub async fn default_tempo(&self) -> Result<f64> {
        Ok(self
            .clients
            .tempo_map
            .get_default_tempo(self.context())
            .await?)
    }

    pub async fn set_default_tempo(&self, bpm: f64) -> Result<()> {
        self.clients
            .tempo_map
            .set_default_tempo(self.context(), bpm)
            .await??;
        Ok(())
    }

    pub async fn default_time_signature(&self) -> Result<(i32, i32)> {
        Ok(self
            .clients
            .tempo_map
            .get_default_time_signature(self.context())
            .await?)
    }

    pub async fn set_default_time_signature(&self, numerator: i32, denominator: i32) -> Result<()> {
        self.clients
            .tempo_map
            .set_default_time_signature(self.context(), numerator, denominator)
            .await??;
        Ok(())
    }

    /// Subscribe to tempo map changes for this project. The server
    /// streams every open project's events; filtering to this
    /// handle's `project_guid` happens client-side. Drop the returned
    /// stream to unsubscribe.
    pub async fn subscribe(
        &self,
    ) -> Result<crate::EventStream<daw_proto::tempo_map::TempoMapStreamEvent>> {
        let (raw_tx, raw_rx) = vox::channel();
        let stream = self.clients.tempo_map_stream.clone();
        let want = self.project_id.clone();
        Ok(crate::EventStream::spawn(
            async move {
                let _ = stream.events(raw_tx).await;
            },
            raw_rx,
            Box::new(move |ev| ev.project_guid == want),
        ))
    }
}

impl std::fmt::Debug for TempoMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TempoMap")
            .field("project_id", &self.project_id)
            .finish()
    }
}
