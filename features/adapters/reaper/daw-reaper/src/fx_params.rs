//! `impl FxParams for Reaper` — sync trait + REAPER C API.
//!
//! Each call resolves the chain through `FxChainContext` against the
//! current project, then indexes into the FX + parameter. Mount via
//! `daw_proto::fx_params::serve(Reaper)`.

use daw_proto::FxParams;
use daw_proto::{DawError, DawResult, FxChainContext, FxParameter};
use reaper_high::{FxChain, Reaper as ReaperHigh, Track};

use crate::fx::build_fx_parameter;
use crate::project_context::find_track_by_guid;

fn resolve_chain(ctx: &FxChainContext) -> Option<(Track, FxChain)> {
    let project = ReaperHigh::get().current_project();
    match ctx {
        FxChainContext::Track(track_guid) => {
            let track = find_track_by_guid(&project, track_guid)?;
            let chain = track.normal_fx_chain();
            Some((track, chain))
        }
        FxChainContext::Input(track_guid) => {
            let track = find_track_by_guid(&project, track_guid)?;
            let chain = track.input_fx_chain();
            Some((track, chain))
        }
        FxChainContext::Monitoring => {
            let track = project.master_track().ok()?;
            let chain = track.input_fx_chain();
            Some((track, chain))
        }
    }
}

impl FxParams for crate::Reaper {
    fn count(&self, ctx: FxChainContext, fx_idx: u32) -> u32 {
        let Some((_track, chain)) = resolve_chain(&ctx) else {
            return 0;
        };
        if fx_idx >= chain.fx_count() {
            return 0;
        }
        let fx = chain.fx_by_index_untracked(fx_idx);
        fx.parameter_count()
    }

    fn get(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32) -> Option<f64> {
        let (_track, chain) = resolve_chain(&ctx)?;
        if fx_idx >= chain.fx_count() {
            return None;
        }
        let fx = chain.fx_by_index_untracked(fx_idx);
        if param_idx >= fx.parameter_count() {
            return None;
        }
        let param = fx.parameter_by_index(param_idx);
        Some(param.reaper_normalized_value().get())
    }

    fn set(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32, value: f64) -> DawResult<()> {
        let (_track, chain) = resolve_chain(&ctx)
            .ok_or_else(|| DawError::not_found("FxChain", &format!("{ctx:?}")))?;
        if fx_idx >= chain.fx_count() {
            return Err(DawError::out_of_range(
                fx_idx,
                chain.fx_count(),
                "fx_params.fx_idx",
            ));
        }
        let fx = chain
            .fx_by_index(fx_idx)
            .ok_or_else(|| DawError::not_found("Fx", &fx_idx.to_string()))?;
        if param_idx >= fx.parameter_count() {
            return Err(DawError::out_of_range(
                param_idx,
                fx.parameter_count(),
                "fx_params.param_idx",
            ));
        }
        let param = fx.parameter_by_index(param_idx);
        let norm = reaper_medium::ReaperNormalizedFxParamValue::new(value);
        param
            .set_reaper_normalized_value(norm)
            .map_err(|e| DawError::operation_failed(format!("set_reaper_normalized_value: {e}")))
    }

    fn name(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32) -> Option<String> {
        let (_track, chain) = resolve_chain(&ctx)?;
        if fx_idx >= chain.fx_count() {
            return None;
        }
        let fx = chain.fx_by_index_untracked(fx_idx);
        if param_idx >= fx.parameter_count() {
            return None;
        }
        let param = fx.parameter_by_index(param_idx);
        param.name().ok().map(|n| n.to_str().to_string())
    }

    fn info(&self, ctx: FxChainContext, fx_idx: u32, param_idx: u32) -> Option<FxParameter> {
        let (_track, chain) = resolve_chain(&ctx)?;
        if fx_idx >= chain.fx_count() {
            return None;
        }
        let fx = chain.fx_by_index_untracked(fx_idx);
        if param_idx >= fx.parameter_count() {
            return None;
        }
        let param = fx.parameter_by_index(param_idx);
        Some(build_fx_parameter(&param))
    }
}
