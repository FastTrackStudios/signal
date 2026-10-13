//! `impl Diagnostics for Standalone` — stub. Standalone has no
//! csurf, so the in-process latency probe returns empty.

use crate::Standalone;
use daw_proto::ProjectContext;
use daw_proto::diagnostics::{
    AudioSyncSnapshot, Diagnostics, DriftDecisionSummary, LocalProjectSnapshot,
    PeerProjectPosition, PeerSummary,
};

impl Diagnostics for Standalone {
    fn hub_publish_latency_us(&self, _project: ProjectContext, _samples: u32) -> Vec<u64> {
        Vec::new()
    }

    fn audio_sync_snapshot(&self) -> Option<AudioSyncSnapshot> {
        None
    }

    fn audio_sync_observe(&self, _count: u32, _interval_us: u64) -> Vec<AudioSyncSnapshot> {
        Vec::new()
    }

    fn audio_sync_peers(&self) -> Vec<PeerSummary> {
        Vec::new()
    }

    fn audio_sync_self_peer_id(&self) -> String {
        String::new()
    }

    fn audio_sync_seed_peer(&self, _peer_id: &str, _addr: &str) -> daw_proto::DawResult<()> {
        Ok(())
    }

    fn audio_sync_drift_decision(&self) -> DriftDecisionSummary {
        DriftDecisionSummary {
            drift_seconds: f64::NAN,
            ..Default::default()
        }
    }

    fn audio_sync_peer_projects(&self, _peer_id: &str) -> Vec<PeerProjectPosition> {
        Vec::new()
    }

    fn audio_sync_local_projects(&self) -> Vec<LocalProjectSnapshot> {
        Vec::new()
    }
}
