//! DawModule implementation for sync.

use daw::module::{ActionDef, DawModule};

pub struct SyncModule;

impl DawModule for SyncModule {
    fn name(&self) -> &str {
        "sync"
    }
    fn display_name(&self) -> &str {
        "Transport Sync"
    }

    fn actions(&self) -> Vec<ActionDef> {
        // TODO: lift the sync action registry once it has a home in this crate.
        // The original implementation pulled definitions from `sync_proto::actions::sync_actions`,
        // which no longer exists. Return empty until the action surface is ported.
        Vec::new()
    }
}

pub fn module() -> Box<dyn DawModule> {
    Box::new(SyncModule)
}
