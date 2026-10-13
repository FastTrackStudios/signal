//! `impl BatchExecution for Standalone` — delegate to the shared
//! program walker.
//!
//! The batch service collapses N method calls into a single RPC by
//! sending an instruction program. `daw_proto::batch::run` walks the
//! program against any backend implementing the covered service
//! traits (Transport/Projects/Tracks/Markers/Effects/Routing) —
//! `Standalone` implements them all, so execution is real here, with
//! full `FromStep` cross-step resolution.

use daw_proto::batch::{BatchExecution, BatchRequest, BatchResponse};

use crate::sync::Standalone;

impl BatchExecution for Standalone {
    fn execute(&self, request: BatchRequest) -> BatchResponse {
        daw_proto::batch::run(self, request)
    }
}
