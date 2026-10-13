//! Batch execution service.
//!
//! Collapses N operations into 1 RPC call by sending a program of
//! instructions that the daw-bridge executes sequentially, resolving
//! cross-step dependencies server-side. Cost: 1 RPC round-trip + N
//! cheap main-thread dispatches.

use super::{BatchRequest, BatchResponse};

#[architect::rpc]
pub trait BatchExecution {
    /// Execute a batch program and return all results.
    fn execute(&self, request: BatchRequest) -> BatchResponse;
}
