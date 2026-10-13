//! `impl BatchExecution for Reaper` — delegate to the shared program
//! walker.
//!
//! Batch execution routes each instruction into the matching service
//! impl (Transport/Projects/Tracks/Markers/Effects/Routing) via
//! `daw_proto::batch::run`, resolving `FromStep` references
//! server-side. The whole `execute` call is one sync method, so when
//! served through the architect bridge it lands on the REAPER main
//! thread once — N instructions, one dispatch.

use daw_proto::batch::{BatchExecution, BatchRequest, BatchResponse};

impl BatchExecution for crate::Reaper {
    fn execute(&self, request: BatchRequest) -> BatchResponse {
        daw_proto::batch::run(self, request)
    }
}
