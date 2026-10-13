//! Whole-FX-chain operations (architect::rpc port).
//!
//! `FxChainContext` already encodes which chain (Track / Input /
//! Monitoring) plus the owning track GUID — per-chain ops are flat
//! method calls keyed by context + fx index. Stateless singleton
//! backends mount via `fx_chains::serve(Reaper)`.

use crate::{DawResult, Fx, FxChainContext};

#[architect::rpc(scopes(chain: FxChainContext))]
pub trait FxChains {
    fn list(&self, ctx: FxChainContext) -> Vec<Fx>;
    fn count(&self, ctx: FxChainContext) -> u32;
    fn get(&self, ctx: FxChainContext, fx_idx: u32) -> Option<Fx>;
    fn name(&self, ctx: FxChainContext, fx_idx: u32) -> Option<String>;

    fn add(&self, ctx: FxChainContext, name: &str) -> DawResult<u32>;
    fn remove(&self, ctx: FxChainContext, fx_idx: u32) -> DawResult<()>;
    fn move_to(&self, ctx: FxChainContext, from_idx: u32, to_idx: u32) -> DawResult<()>;
    fn rename(&self, ctx: FxChainContext, fx_idx: u32, name: &str) -> DawResult<()>;

    fn set_enabled(&self, ctx: FxChainContext, fx_idx: u32, enabled: bool) -> DawResult<()>;
    fn set_online(&self, ctx: FxChainContext, fx_idx: u32, online: bool) -> DawResult<()>;
    fn set_show_ui(&self, ctx: FxChainContext, fx_idx: u32, show: bool) -> DawResult<()>;

    fn state_chunk(&self, ctx: FxChainContext, fx_idx: u32) -> Option<String>;
    fn set_state_chunk(&self, ctx: FxChainContext, fx_idx: u32, chunk: &str) -> DawResult<()>;
}
