//! Shared state for the old `StandaloneProject` / `StandaloneTransport`
//! service pair. Both services have been ported to architect::rpc and
//! their service-struct shells retired, but `SharedProjectState`
//! survives for the `Daw`-level facade that still tracks "current
//! project" by index. Future port: fold this into `StandaloneState`
//! directly.

use crate::platform::RwLock;
use std::sync::Arc;

/// Resolves "current" project GUID across the legacy standalone
/// project/transport surfaces.
#[derive(Clone)]
pub struct SharedProjectState {
    /// GUIDs of all available projects.
    pub project_guids: Arc<Vec<String>>,
    /// Index of the currently selected project.
    pub current_index: Arc<RwLock<usize>>,
}

impl SharedProjectState {
    pub fn new(project_guids: Vec<String>) -> Self {
        Self {
            project_guids: Arc::new(project_guids),
            current_index: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn current_guid(&self) -> Option<String> {
        let index = *self.current_index.read().await;
        self.project_guids.get(index).cloned()
    }
}
