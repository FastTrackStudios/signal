//! FX parameter operations (architect::rpc port).
//!
//! Per-parameter ops on a specific FX in a chain. `FxChainContext`
//! carries the owning track GUID; `fx_idx` + `param_idx` identify
//! the target parameter. Mount via `fx_params::serve(Reaper)`.

use crate::{DawResult, FxChainContext, FxParameter};

#[architect::rpc]
pub trait FxParams {
    fn count(&self, ctx: FxChainContext, fx_idx: u32) -> u32;

    /// Normalized parameter value (0.0 - 1.0).
    fn get(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32) -> Option<f64>;
    /// Set normalized parameter value (0.0 - 1.0).
    fn set(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32, value: f64) -> DawResult<()>;

    fn name(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32) -> Option<String>;
    fn info(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32) -> Option<FxParameter>;
}
