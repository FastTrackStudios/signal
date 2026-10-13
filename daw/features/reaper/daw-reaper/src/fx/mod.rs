//! `impl Effects for Reaper` — REAPER FX service.
//!
//! Layout:
//!
//! - this file: trait impl, sync method bodies that talk to REAPER
//!   directly (the `HasDispatcher` machinery puts us on the main thread).
//! - [`util`]: project/chain/index resolution, FX/FxParameter builders.
//! - [`tree`]: container-aware tree construction + node-id resolution.
//! - [`state`]: state-chunk capture/restore helpers (per-FX and chain).
//!
//! ## Reference implementations
//!
//! Informed by two REAPER scripts:
//!
//! - **Snapshooter** (tilr): per-parameter snapshots via FX GUID + index.
//! - **Track Snapshot** (Daniel Lumertz): chunk-level capture/restore via
//!   GetTrackStateChunk + selective section swapping.

mod state;
mod tree;
mod util;

use daw_proto::{
    AddFxAtRequest, CreateContainerRequest, DawError, DawResult, Effects,
    EncloseInContainerRequest, Fx, FxChainContext, FxChannelConfig, FxContainerChannelConfig,
    FxLatency, FxNodeId, FxParamModulation, FxParameter, FxPinMappings, FxPresetIndex,
    FxRoutingMode, FxStateChunk, FxTarget, FxTree, InstalledFx, LastTouchedFx,
    MoveFromContainerRequest, MoveToContainerRequest, ProjectContext,
    SetContainerChannelConfigRequest, SetNamedConfigRequest, SetParameterByNameRequest,
    SetParameterRequest,
};
use reaper_high::{MAX_TRACK_CHUNK_SIZE, Reaper};
use reaper_medium::{ChunkCacheHint, FxPresetRef, TrackFxLocation, TransferBehavior};
use tracing::{info, warn};

use crate::safe_wrappers::fx as fx_sw;

// Re-export helpers used by other modules in this crate (batch
// dispatch, fx_chains, fx_params).
pub use state::{
    capture_fx_state_recursive, find_block_end, get_fx_block_via_track_chunk,
    set_fx_block_via_track_chunk,
};
pub use tree::{
    CONTAINER_BASE, build_fx_tree_from_chain, container_child_fx_id, is_container_fx,
    resolve_node_to_raw_index, resolve_plugin_guid, verify_container_child_count,
};
pub use util::{
    build_fx_info, build_fx_parameter, fx_location, parse_fx_type, read_config_f64,
    read_config_i32, read_config_str, read_config_u32, resolve_fx_chain, resolve_fx_index,
    safe_fx_call,
};

use util::resolve_project;

impl Effects for crate::Reaper {
    // ── Installed plugins ──────────────────────────────────────────────

    fn list_installed(&self) -> Vec<InstalledFx> {
        let reaper = Reaper::get();
        let low = reaper.medium_reaper().low();
        let mut results = Vec::new();
        let mut index = 0i32;
        while let Some((name, ident)) = fx_sw::enum_installed_fx(low, index) {
            results.push(InstalledFx { name, ident });
            index += 1;
        }
        results
    }

    fn last_touched(&self) -> Option<LastTouchedFx> {
        let reaper = Reaper::get();
        let result = reaper.medium_reaper().get_last_touched_fx()?;

        use reaper_medium::GetLastTouchedFxResult::*;
        match result {
            TrackFx {
                track_location,
                fx_location,
                param_index,
            } => {
                let project = reaper.current_project();
                let track = match track_location {
                    reaper_medium::TrackLocation::MasterTrack => project.master_track().ok()?,
                    reaper_medium::TrackLocation::NormalTrack(idx) => {
                        project.track_by_index(idx)?
                    }
                };
                let track_guid = track.guid().to_string_without_braces();
                let (fx_index, is_input_fx) = match fx_location {
                    TrackFxLocation::NormalFxChain(idx) => (idx, false),
                    TrackFxLocation::InputFxChain(idx) => (idx, true),
                    _ => return None,
                };
                Some(LastTouchedFx {
                    track_guid,
                    fx_index,
                    param_index,
                    is_input_fx,
                })
            }
            _ => None,
        }
    }

    // ── Chain queries ──────────────────────────────────────────────────

    fn list(&self, project: ProjectContext, chain: FxChainContext) -> Vec<Fx> {
        let Some(proj) = resolve_project(&project) else {
            warn!("list: project not found ({:?})", project);
            return vec![];
        };
        let Some((_track, fx_chain)) = resolve_fx_chain(&proj, &chain) else {
            warn!("list: FX chain not found ({:?})", chain);
            return vec![];
        };
        let chain_ref = &fx_chain;
        fx_chain
            .fxs()
            .map(|fx| build_fx_info(&fx, Some(chain_ref)))
            .collect()
    }

    fn get(&self, project: ProjectContext, target: FxTarget) -> Option<Fx> {
        let proj = resolve_project(&project)?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context)?;
        let index = resolve_fx_index(&chain, &target.fx)?;
        let fx = chain.fx_by_index_untracked(index);
        Some(build_fx_info(&fx, Some(&chain)))
    }

    fn count(&self, project: ProjectContext, chain: FxChainContext) -> u32 {
        let Some(proj) = resolve_project(&project) else {
            return 0;
        };
        let Some((_track, fx_chain)) = resolve_fx_chain(&proj, &chain) else {
            return 0;
        };
        fx_chain.fx_count()
    }

    // ── State ──────────────────────────────────────────────────────────

    fn set_enabled(
        &self,
        project: ProjectContext,
        target: FxTarget,
        enabled: bool,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found for {:?}", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx).ok_or_else(|| {
            DawError::operation_failed(format!("FX not found for {:?}", target.fx))
        })?;
        let fx = chain.fx_by_index_untracked(index);
        if enabled {
            fx.enable()
                .map_err(|e| DawError::operation_failed(format!("enable failed: {}", e)))?;
        } else {
            fx.disable()
                .map_err(|e| DawError::operation_failed(format!("disable failed: {}", e)))?;
        }
        Ok(())
    }

    fn set_offline(
        &self,
        project: ProjectContext,
        target: FxTarget,
        offline: bool,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found for {:?}", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx).ok_or_else(|| {
            DawError::operation_failed(format!("FX not found for {:?}", target.fx))
        })?;
        let fx = chain.fx_by_index_untracked(index);
        fx.set_online(!offline)
            .map_err(|e| DawError::operation_failed(format!("set_online failed: {}", e)))?;
        Ok(())
    }

    // ── Management ─────────────────────────────────────────────────────

    fn add(&self, project: ProjectContext, chain: FxChainContext, name: &str) -> Option<String> {
        let proj = resolve_project(&project)?;
        let (_track, fx_chain) = resolve_fx_chain(&proj, &chain)?;
        let fx = fx_chain.add_fx_by_original_name(name)?;
        let guid = fx.get_or_query_guid().ok()?;
        Some(guid.to_string_without_braces())
    }

    fn add_at(&self, project: ProjectContext, request: AddFxAtRequest) -> Option<String> {
        let proj = resolve_project(&project)?;
        let (_track, chain) = resolve_fx_chain(&proj, &request.context)?;
        let fx = chain.add_fx_by_original_name(request.name.as_str())?;
        let guid = fx.get_or_query_guid().ok()?;
        let current_index = fx.index();
        if current_index != request.index {
            let is_input = chain.is_input_fx();
            let track = chain.track()?;
            let raw_track = track.raw().ok()?;
            fx_sw::track_fx_copy_to_track(
                Reaper::get().medium_reaper(),
                (raw_track, fx_location(current_index, is_input)),
                (raw_track, fx_location(request.index, is_input)),
                TransferBehavior::Move,
            );
        }
        Some(guid.to_string_without_braces())
    }

    fn remove(&self, project: ProjectContext, target: FxTarget) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found for {:?}", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx).ok_or_else(|| {
            DawError::operation_failed(format!("FX not found for {:?}", target.fx))
        })?;
        let raw_track = track
            .raw()
            .map_err(|e| DawError::operation_failed(format!("raw track failed: {}", e)))?;
        let location = fx_location(index, chain.is_input_fx());
        fx_sw::track_fx_delete(Reaper::get().medium_reaper(), raw_track, location)
            .map_err(|e| DawError::operation_failed(format!("track_fx_delete failed: {}", e)))?;
        Ok(())
    }

    fn move_to(&self, project: ProjectContext, target: FxTarget, new_index: u32) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found for {:?}", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx).ok_or_else(|| {
            DawError::operation_failed(format!("FX not found for {:?}", target.fx))
        })?;
        let raw_track = track
            .raw()
            .map_err(|e| DawError::operation_failed(format!("raw track failed: {}", e)))?;
        let is_input = chain.is_input_fx();
        fx_sw::track_fx_copy_to_track(
            Reaper::get().medium_reaper(),
            (raw_track, fx_location(index, is_input)),
            (raw_track, fx_location(new_index, is_input)),
            TransferBehavior::Move,
        );
        Ok(())
    }

    // ── Parameters ─────────────────────────────────────────────────────

    fn parameters(&self, project: ProjectContext, target: FxTarget) -> Vec<FxParameter> {
        let Some(proj) = resolve_project(&project) else {
            return vec![];
        };
        let Some((_track, chain)) = resolve_fx_chain(&proj, &target.context) else {
            return vec![];
        };
        let Some(index) = resolve_fx_index(&chain, &target.fx) else {
            return vec![];
        };
        let fx = chain.fx_by_index_untracked(index);
        fx.parameters()
            .map(|param| build_fx_parameter(&param))
            .collect()
    }

    fn parameter(
        &self,
        project: ProjectContext,
        target: FxTarget,
        index: u32,
    ) -> Option<FxParameter> {
        let proj = resolve_project(&project)?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context)?;
        let fx_idx = resolve_fx_index(&chain, &target.fx)?;
        let fx = chain.fx_by_index_untracked(fx_idx);
        if index >= fx.parameter_count() {
            return None;
        }
        let param = fx.parameter_by_index(index);
        Some(build_fx_parameter(&param))
    }

    fn set_parameter(
        &self,
        project: ProjectContext,
        request: SetParameterRequest,
    ) -> DawResult<()> {
        // CLAP plugins only receive parameter changes during audio
        // processing. Without a running engine, set_reaper_normalized_value
        // appears to succeed but the plugin never sees the change.
        let reaper = Reaper::get();
        if !reaper.medium_reaper().audio_is_running() {
            return Err(DawError::operation_failed(
                "audio engine is not running — parameter changes won't reach CLAP plugins. \
                 Start playback or enable monitoring first.",
            ));
        }
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, chain) =
            resolve_fx_chain(&proj, &request.target.context).ok_or_else(|| {
                DawError::operation_failed(format!(
                    "FX chain not found for {:?}",
                    request.target.context
                ))
            })?;
        let fx_idx = resolve_fx_index(&chain, &request.target.fx).ok_or_else(|| {
            DawError::operation_failed(format!("FX not found for {:?}", request.target.fx))
        })?;
        let fx = chain.fx_by_index(fx_idx).ok_or_else(|| {
            DawError::operation_failed(format!("fx_by_index({}) returned None", fx_idx))
        })?;
        let param = fx.parameter_by_index(request.index);
        let norm_val = reaper_medium::ReaperNormalizedFxParamValue::new(request.value);
        param.set_reaper_normalized_value(norm_val).map_err(|e| {
            DawError::operation_failed(format!("set_reaper_normalized_value failed: {e}"))
        })?;
        Ok(())
    }

    fn parameter_by_name(
        &self,
        project: ProjectContext,
        target: FxTarget,
        name: &str,
    ) -> Option<FxParameter> {
        let proj = resolve_project(&project)?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context)?;
        let fx_idx = resolve_fx_index(&chain, &target.fx)?;
        let fx = chain.fx_by_index_untracked(fx_idx);
        for param in fx.parameters() {
            if let Ok(pname) = param.name()
                && pname.to_str() == name
            {
                return Some(build_fx_parameter(&param));
            }
        }
        None
    }

    fn set_parameter_by_name(
        &self,
        project: ProjectContext,
        request: SetParameterByNameRequest,
    ) -> DawResult<()> {
        let reaper = Reaper::get();
        if !reaper.medium_reaper().audio_is_running() {
            return Err(DawError::operation_failed(
                "audio engine is not running — parameter changes won't reach CLAP plugins. \
                 Start playback or enable monitoring first.",
            ));
        }
        let proj = resolve_project(&project)
            .ok_or_else(|| DawError::not_found("project", &format!("{:?}", project)))?;
        let (_track, chain) =
            resolve_fx_chain(&proj, &request.target.context).ok_or_else(|| {
                DawError::operation_failed(format!(
                    "FX chain not found ({:?})",
                    request.target.context
                ))
            })?;
        let fx_idx = resolve_fx_index(&chain, &request.target.fx).ok_or_else(|| {
            DawError::operation_failed(format!("FX not found ({:?})", request.target.fx))
        })?;
        let fx = chain.fx_by_index(fx_idx).ok_or_else(|| {
            DawError::operation_failed(format!("fx_by_index({}) returned None", fx_idx))
        })?;
        for param in fx.parameters() {
            if let Ok(pname) = param.name()
                && pname.to_str() == request.name
            {
                let value = reaper_medium::ReaperNormalizedFxParamValue::new(request.value);
                param.set_reaper_normalized_value(value).map_err(|e| {
                    DawError::operation_failed(format!(
                        "set_reaper_normalized_value failed for {:?}: {e}",
                        request.name
                    ))
                })?;
                return Ok(());
            }
        }
        Err(DawError::not_found("parameter", &request.name))
    }

    // ── Presets ────────────────────────────────────────────────────────

    fn preset_index(&self, project: ProjectContext, target: FxTarget) -> Option<FxPresetIndex> {
        let proj = resolve_project(&project)?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context)?;
        let index = resolve_fx_index(&chain, &target.fx)?;
        let fx = chain.fx_by_index_untracked(index);
        let result = safe_fx_call("fx.preset_index_and_count", None, || {
            fx.preset_index_and_count()
        })?;
        let name = safe_fx_call("fx.preset_name", None, || {
            fx.preset_name()
                .map(|rs| rs.to_str().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or(None);
        Some(FxPresetIndex {
            index: result.index,
            count: result.count,
            name,
        })
    }

    fn next_preset(&self, project: ProjectContext, target: FxTarget) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx)
            .ok_or_else(|| DawError::operation_failed(format!("FX not found ({:?})", target.fx)))?;
        let raw_track = track
            .raw()
            .map_err(|_| DawError::operation_failed("raw track not available"))?;
        let location = fx_location(index, chain.is_input_fx());
        fx_sw::track_fx_navigate_presets(Reaper::get().medium_reaper(), raw_track, location, 1);
        Ok(())
    }

    fn prev_preset(&self, project: ProjectContext, target: FxTarget) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx)
            .ok_or_else(|| DawError::operation_failed(format!("FX not found ({:?})", target.fx)))?;
        let raw_track = track
            .raw()
            .map_err(|_| DawError::operation_failed("raw track not available"))?;
        let location = fx_location(index, chain.is_input_fx());
        fx_sw::track_fx_navigate_presets(Reaper::get().medium_reaper(), raw_track, location, -1);
        Ok(())
    }

    fn set_preset(&self, project: ProjectContext, target: FxTarget, index: u32) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", target.context))
        })?;
        let fx_idx = resolve_fx_index(&chain, &target.fx)
            .ok_or_else(|| DawError::operation_failed(format!("FX not found ({:?})", target.fx)))?;
        let raw_track = track
            .raw()
            .map_err(|e| DawError::operation_failed(format!("raw track failed: {}", e)))?;
        let location = fx_location(fx_idx, chain.is_input_fx());
        fx_sw::track_fx_set_preset_by_index(
            Reaper::get().medium_reaper(),
            raw_track,
            location,
            FxPresetRef::Preset(index),
        );
        Ok(())
    }

    // ── UI ─────────────────────────────────────────────────────────────

    fn open_ui(&self, project: ProjectContext, target: FxTarget) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx)
            .ok_or_else(|| DawError::operation_failed(format!("FX not found ({:?})", target.fx)))?;
        let fx = chain.fx_by_index_untracked(index);
        let _ = fx.show_in_floating_window();
        Ok(())
    }

    fn close_ui(&self, project: ProjectContext, target: FxTarget) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx)
            .ok_or_else(|| DawError::operation_failed(format!("FX not found ({:?})", target.fx)))?;
        let fx = chain.fx_by_index_untracked(index);
        let _ = fx.hide_floating_window();
        Ok(())
    }

    fn toggle_ui(&self, project: ProjectContext, target: FxTarget) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx)
            .ok_or_else(|| DawError::operation_failed(format!("FX not found ({:?})", target.fx)))?;
        let fx = chain.fx_by_index_untracked(index);
        if fx.window_is_open() {
            let _ = fx.hide_floating_window();
        } else {
            let _ = fx.show_in_floating_window();
        }
        Ok(())
    }

    // ── Named config ───────────────────────────────────────────────────

    fn named_config(&self, project: ProjectContext, target: FxTarget, key: &str) -> Option<String> {
        let proj = resolve_project(&project)?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context)?;
        let index = resolve_fx_index(&chain, &target.fx)?;
        let fx = chain.fx_by_index_untracked(index);
        let bytes = fx.get_named_config_param(key, 4096).ok()?;
        Some(String::from_utf8_lossy(&bytes).to_string())
    }

    fn set_named_config(
        &self,
        project: ProjectContext,
        request: SetNamedConfigRequest,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, chain) =
            resolve_fx_chain(&proj, &request.target.context).ok_or_else(|| {
                DawError::operation_failed(format!(
                    "FX chain not found ({:?})",
                    request.target.context
                ))
            })?;
        let index = resolve_fx_index(&chain, &request.target.fx).ok_or_else(|| {
            DawError::operation_failed(format!("FX not found ({:?})", request.target.fx))
        })?;
        let fx = chain.fx_by_index_untracked(index);
        fx_sw::fx_set_named_config_param(&fx, &request.key, &request.value)
            .map_err(DawError::operation_failed)?;
        Ok(())
    }

    fn latency(&self, project: ProjectContext, target: FxTarget) -> Option<FxLatency> {
        let proj = resolve_project(&project)?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context)?;
        let index = resolve_fx_index(&chain, &target.fx)?;
        let fx = chain.fx_by_index_untracked(index);
        let pdc_samples = read_config_i32(&fx, "pdc");
        let chain_pdc_actual = read_config_i32(&fx, "chain_pdc_actual");
        let chain_pdc_reporting = read_config_i32(&fx, "chain_pdc_reporting");
        Some(FxLatency {
            pdc_samples,
            chain_pdc_actual,
            chain_pdc_reporting,
        })
    }

    fn param_modulation(
        &self,
        project: ProjectContext,
        target: FxTarget,
        param_index: u32,
    ) -> Option<FxParamModulation> {
        let proj = resolve_project(&project)?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context)?;
        let index = resolve_fx_index(&chain, &target.fx)?;
        let fx = chain.fx_by_index_untracked(index);

        let p = param_index;
        let lfo_active =
            read_config_str(&fx, &format!("param.{p}.lfo.active")).is_some_and(|s| s == "1");
        let lfo_speed = read_config_f64(&fx, &format!("param.{p}.lfo.speed"));
        let lfo_strength = read_config_f64(&fx, &format!("param.{p}.lfo.strength"));

        let link_active =
            read_config_str(&fx, &format!("param.{p}.plink.active")).is_some_and(|s| s == "1");
        let link_fx_index = read_config_i32(&fx, &format!("param.{p}.plink.effect"));
        let link_param_index = read_config_i32(&fx, &format!("param.{p}.plink.param"));

        Some(FxParamModulation {
            lfo_active,
            lfo_speed,
            lfo_strength,
            link_active,
            link_fx_index,
            link_param_index,
        })
    }

    // ── Channel config ─────────────────────────────────────────────────

    fn channel_config(&self, project: ProjectContext, target: FxTarget) -> Option<FxChannelConfig> {
        let proj = resolve_project(&project)?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context)?;
        let index = resolve_fx_index(&chain, &target.fx)?;
        let fx = chain.fx_by_index_untracked(index);

        let config = match read_config_str(&fx, "channel_config") {
            Some(raw) if !raw.is_empty() => {
                let parts: Vec<&str> = raw.split_whitespace().collect();
                FxChannelConfig {
                    channel_count: parts.first().and_then(|s| s.parse().ok()).unwrap_or(0),
                    channel_mode: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
                    supported_flags: parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
                }
            }
            _ => FxChannelConfig::default(),
        };
        Some(config)
    }

    fn set_channel_config(
        &self,
        project: ProjectContext,
        target: FxTarget,
        config: FxChannelConfig,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx)
            .ok_or_else(|| DawError::operation_failed(format!("FX not found ({:?})", target.fx)))?;
        let fx = chain.fx_by_index_untracked(index);
        let value = format!("{} {}", config.channel_count, config.channel_mode);
        fx_sw::fx_set_named_config_param(&fx, "channel_config", &value)
            .map_err(DawError::operation_failed)?;
        Ok(())
    }

    // ── Output pin mappings (silence/restore) ──────────────────────────

    fn silence_output(
        &self,
        project: ProjectContext,
        target: FxTarget,
    ) -> DawResult<FxPinMappings> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx)
            .ok_or_else(|| DawError::operation_failed(format!("FX not found ({:?})", target.fx)))?;
        let raw_track = track
            .raw()
            .map_err(|e| DawError::operation_failed(format!("raw track failed: {}", e)))?;
        let is_input = chain.is_input_fx();
        let raw_fx = fx_location(index, is_input).to_raw();
        let low = Reaper::get().medium_reaper().low();

        const MAX_PINS: i32 = 64;
        const SECOND_BANK: i32 = 0x1000000;
        let mut saved = Vec::new();

        for pin in 0..MAX_PINS {
            let (low32, high32) = fx_sw::track_fx_get_pin_mappings(low, raw_track, raw_fx, 1, pin);
            if low32 != 0 || high32 != 0 {
                saved.push((pin, low32, high32));
                fx_sw::track_fx_set_pin_mappings(low, raw_track, raw_fx, 1, pin, 0, 0);
            }
        }
        for pin in 0..MAX_PINS {
            let (low32, high32) =
                fx_sw::track_fx_get_pin_mappings(low, raw_track, raw_fx, 1, pin + SECOND_BANK);
            if low32 != 0 || high32 != 0 {
                saved.push((pin + SECOND_BANK, low32, high32));
                fx_sw::track_fx_set_pin_mappings(
                    low,
                    raw_track,
                    raw_fx,
                    1,
                    pin + SECOND_BANK,
                    0,
                    0,
                );
            }
        }

        Ok(FxPinMappings { output_pins: saved })
    }

    fn restore_output(
        &self,
        project: ProjectContext,
        target: FxTarget,
        saved: FxPinMappings,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx)
            .ok_or_else(|| DawError::operation_failed(format!("FX not found ({:?})", target.fx)))?;
        let raw_track = track
            .raw()
            .map_err(|e| DawError::operation_failed(format!("raw track failed: {}", e)))?;
        let is_input = chain.is_input_fx();
        let raw_fx = fx_location(index, is_input).to_raw();
        let low = Reaper::get().medium_reaper().low();

        if saved.output_pins.is_empty() {
            // Default stereo pass-through: pin 0 → ch 0, pin 1 → ch 1.
            fx_sw::track_fx_set_pin_mappings(low, raw_track, raw_fx, 1, 0, 1, 0);
            fx_sw::track_fx_set_pin_mappings(low, raw_track, raw_fx, 1, 1, 2, 0);
        } else {
            for &(pin, low32, high32) in &saved.output_pins {
                fx_sw::track_fx_set_pin_mappings(low, raw_track, raw_fx, 1, pin, low32, high32);
            }
        }
        Ok(())
    }

    // ── State chunks ───────────────────────────────────────────────────

    fn state_chunk(&self, project: ProjectContext, target: FxTarget) -> Option<Vec<u8>> {
        let proj = resolve_project(&project)?;
        let (track, chain) = resolve_fx_chain(&proj, &target.context)?;
        let index = resolve_fx_index(&chain, &target.fx)?;
        let fx = chain.fx_by_index_untracked(index);

        match fx.tag_chunk() {
            Ok(tag) => Some(tag.content().to_string().into_bytes()),
            Err(_) => get_fx_block_via_track_chunk(&track, index).map(|s| s.into_bytes()),
        }
    }

    fn set_state_chunk(
        &self,
        project: ProjectContext,
        target: FxTarget,
        chunk: Vec<u8>,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx)
            .ok_or_else(|| DawError::operation_failed(format!("FX not found ({:?})", target.fx)))?;
        let fx = chain.fx_by_index_untracked(index);

        let chunk_str = String::from_utf8(chunk)
            .map_err(|e| DawError::operation_failed(format!("chunk is not valid UTF-8: {e}")))?;

        match fx.set_tag_chunk(&chunk_str) {
            Ok(()) => Ok(()),
            Err(_) => set_fx_block_via_track_chunk(&track, index, &chunk_str)
                .map_err(DawError::operation_failed),
        }
    }

    fn state_chunk_encoded(&self, project: ProjectContext, target: FxTarget) -> Option<String> {
        let proj = resolve_project(&project)?;
        let (track, chain) = resolve_fx_chain(&proj, &target.context)?;
        let index = resolve_fx_index(&chain, &target.fx)?;
        let fx = chain.fx_by_index_untracked(index);

        match fx.tag_chunk() {
            Ok(tag) => Some(tag.content().to_string()),
            Err(_) => get_fx_block_via_track_chunk(&track, index),
        }
    }

    fn set_state_chunk_encoded(
        &self,
        project: ProjectContext,
        target: FxTarget,
        encoded: &str,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (track, chain) = resolve_fx_chain(&proj, &target.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", target.context))
        })?;
        let index = resolve_fx_index(&chain, &target.fx)
            .ok_or_else(|| DawError::operation_failed(format!("FX not found ({:?})", target.fx)))?;
        let fx = chain.fx_by_index_untracked(index);

        match fx.set_tag_chunk(encoded) {
            Ok(()) => Ok(()),
            Err(_) => set_fx_block_via_track_chunk(&track, index, encoded)
                .map_err(DawError::operation_failed),
        }
    }

    fn chain_state(&self, project: ProjectContext, chain: FxChainContext) -> Vec<FxStateChunk> {
        let Some(proj) = resolve_project(&project) else {
            return vec![];
        };
        let Some((track, fx_chain)) = resolve_fx_chain(&proj, &chain) else {
            return vec![];
        };
        let fx_count = fx_chain.fx_count();
        let mut chunks = Vec::new();
        capture_fx_state_recursive(&fx_chain, &track, fx_count, &mut chunks);
        chunks
    }

    fn set_chain_state(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        chunks: Vec<FxStateChunk>,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (track, fx_chain) = resolve_fx_chain(&proj, &chain).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", chain))
        })?;

        // GUID → index map (Snapshooter-style); also name → indices for
        // cross-track / cross-template fallback when GUIDs don't match.
        let mut guid_to_index = std::collections::HashMap::new();
        let mut name_to_indices: std::collections::HashMap<String, Vec<u32>> =
            std::collections::HashMap::new();
        for fx in fx_chain.fxs() {
            if let Ok(guid) = fx.get_or_query_guid() {
                guid_to_index.insert(guid.to_string_without_braces(), fx.index());
            }
            let fx_name = safe_fx_call("fx.name", None, || fx.name().to_str().to_string())
                .unwrap_or_default();
            name_to_indices.entry(fx_name).or_default().push(fx.index());
        }
        let mut name_consumed: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        let set_chunk_with_fallback = |fx_index: u32, encoded: &str| {
            if let Some(fx) = fx_chain.fx_by_index(fx_index) {
                match fx.set_tag_chunk(encoded) {
                    Ok(()) => {}
                    Err(_) => {
                        if let Err(e) = set_fx_block_via_track_chunk(&track, fx_index, encoded) {
                            warn!(
                                "set_fx_block_via_track_chunk also failed for FX[{}]: {}",
                                fx_index, e
                            );
                        }
                    }
                }
            }
        };

        for chunk in &chunks {
            if let Some(&fx_index) = guid_to_index.get(&chunk.fx_guid) {
                set_chunk_with_fallback(fx_index, &chunk.encoded_chunk);
            } else {
                let consumed = name_consumed.entry(chunk.plugin_name.clone()).or_insert(0);
                if let Some(indices) = name_to_indices.get(&chunk.plugin_name) {
                    if let Some(&fx_index) = indices.get(*consumed) {
                        info!(
                            "Cross-track match: '{}' GUID {} → index {} (by name, pos {})",
                            chunk.plugin_name, chunk.fx_guid, fx_index, consumed
                        );
                        set_chunk_with_fallback(fx_index, &chunk.encoded_chunk);
                        *consumed += 1;
                    } else {
                        warn!(
                            "FX '{}' has no more unmatched instances (pos {})",
                            chunk.plugin_name, consumed
                        );
                    }
                } else {
                    warn!(
                        "FX '{}' (GUID {}) not found by name either",
                        chunk.plugin_name, chunk.fx_guid
                    );
                }
            }
        }
        Ok(())
    }

    // ── Containers / tree ──────────────────────────────────────────────

    fn tree(&self, project: ProjectContext, chain: FxChainContext) -> FxTree {
        let Some(proj) = resolve_project(&project) else {
            return FxTree::new();
        };
        let Some((_track, fx_chain)) = resolve_fx_chain(&proj, &chain) else {
            return FxTree::new();
        };
        let is_input = matches!(chain, FxChainContext::Input(_) | FxChainContext::Monitoring);
        let top_level_count = fx_chain.fx_count();
        if top_level_count == 0 {
            return FxTree::new();
        }
        let nodes = build_fx_tree_from_chain(&fx_chain, is_input, top_level_count);
        FxTree::from_nodes(nodes)
    }

    fn create_container(
        &self,
        project: ProjectContext,
        request: CreateContainerRequest,
    ) -> Option<FxNodeId> {
        let proj = resolve_project(&project)?;
        let (_track, chain) = resolve_fx_chain(&proj, &request.context)?;
        let is_input = chain.is_input_fx();
        let track = chain.track()?;
        let raw_track = track.raw().ok()?;

        let container_fx = chain.insert_fx_by_name(chain.fx_count(), "Container")?;
        let container_index = container_fx.index();

        if container_index != request.index && request.index < chain.fx_count() {
            fx_sw::track_fx_copy_to_track(
                Reaper::get().medium_reaper(),
                (raw_track, fx_location(container_index, is_input)),
                (raw_track, fx_location(request.index, is_input)),
                TransferBehavior::Move,
            );
        }

        let final_index = if container_index != request.index && request.index < chain.fx_count() {
            request.index
        } else {
            container_index
        };
        let container_fx = chain.fx_by_index_untracked(final_index);
        let _ = fx_sw::fx_set_named_config_param(&container_fx, "renamed_name", &request.name);

        Some(FxNodeId::container(format!("{}", final_index)))
    }

    fn move_to_container(
        &self,
        project: ProjectContext,
        request: MoveToContainerRequest,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, chain) = resolve_fx_chain(&proj, &request.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", request.context))
        })?;
        let is_input = chain.is_input_fx();
        let track = chain
            .track()
            .ok_or_else(|| DawError::operation_failed("track not found"))?;
        let raw_track = track
            .raw()
            .map_err(|e| DawError::operation_failed(format!("raw track failed: {}", e)))?;

        let source_raw = resolve_node_to_raw_index(&chain, &request.node_id).ok_or_else(|| {
            DawError::operation_failed(format!(
                "could not resolve source node {:?}",
                request.node_id
            ))
        })?;
        let container_raw =
            resolve_node_to_raw_index(&chain, &request.container_id).ok_or_else(|| {
                DawError::operation_failed(format!(
                    "could not resolve container {:?}",
                    request.container_id
                ))
            })?;

        // Two-step pattern (MPL Multi-Mono Container):
        //  1. Move FX to container's first child slot (always valid)
        //  2. Shuffle to target position inside the container if needed
        let container_fx = chain.fx_by_index_untracked(container_raw);
        let child_count = read_config_u32(&container_fx, "container_count");
        let child_pos = request.child_index.min(child_count);

        let first_slot = if child_count > 0 {
            container_child_fx_id(&container_fx, 0)
                .ok_or_else(|| DawError::operation_failed("container_item.0 not found"))?
        } else {
            let top_count = chain.fx_count();
            let stride = top_count + 1;
            CONTAINER_BASE + container_raw + stride
        };

        fx_sw::track_fx_copy_to_track(
            Reaper::get().medium_reaper(),
            (raw_track, fx_location(source_raw, is_input)),
            (raw_track, fx_location(first_slot, is_input)),
            TransferBehavior::Move,
        );

        verify_container_child_count(&chain, container_raw, child_count + 1)
            .map_err(DawError::operation_failed)?;

        if child_pos > 0 {
            let container_fx = chain.fx_by_index_untracked(container_raw);
            let new_count = read_config_u32(&container_fx, "container_count");
            let src = container_child_fx_id(&container_fx, 0).ok_or_else(|| {
                DawError::operation_failed("container_item.0 not found after move")
            })?;
            let target_pos = child_pos.min(new_count - 1);
            let dest = container_child_fx_id(&container_fx, target_pos).ok_or_else(|| {
                DawError::operation_failed(format!("container_item.{} not found", target_pos))
            })?;

            fx_sw::track_fx_copy_to_track(
                Reaper::get().medium_reaper(),
                (raw_track, fx_location(src, is_input)),
                (raw_track, fx_location(dest, is_input)),
                TransferBehavior::Move,
            );

            verify_container_child_count(&chain, container_raw, child_count + 1)
                .map_err(DawError::operation_failed)?;
        }

        Ok(())
    }

    fn move_from_container(
        &self,
        project: ProjectContext,
        request: MoveFromContainerRequest,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, chain) = resolve_fx_chain(&proj, &request.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", request.context))
        })?;
        let is_input = chain.is_input_fx();
        let track = chain
            .track()
            .ok_or_else(|| DawError::operation_failed("track not found"))?;
        let raw_track = track
            .raw()
            .map_err(|e| DawError::operation_failed(format!("raw track failed: {}", e)))?;

        let source_raw = resolve_node_to_raw_index(&chain, &request.node_id).ok_or_else(|| {
            DawError::operation_failed(format!("could not resolve node {:?}", request.node_id))
        })?;

        let top_count_before = chain.fx_count();

        fx_sw::track_fx_copy_to_track(
            Reaper::get().medium_reaper(),
            (raw_track, fx_location(source_raw, is_input)),
            (raw_track, fx_location(request.target_index, is_input)),
            TransferBehavior::Move,
        );

        let top_count_after = chain.fx_count();
        if top_count_after != top_count_before + 1 {
            return Err(DawError::operation_failed(format!(
                "move_from_container: top-level FX count {} -> {} (expected {})",
                top_count_before,
                top_count_after,
                top_count_before + 1
            )));
        }
        Ok(())
    }

    fn set_routing_mode(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        node_id: FxNodeId,
        mode: FxRoutingMode,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, fx_chain) = resolve_fx_chain(&proj, &chain).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", chain))
        })?;
        let raw_index = resolve_node_to_raw_index(&fx_chain, &node_id).ok_or_else(|| {
            DawError::operation_failed(format!("could not resolve container ({:?})", node_id))
        })?;
        let container_fx = fx_chain.fx_by_index_untracked(raw_index);
        let value = mode.to_reaper_param();
        fx_sw::fx_set_named_config_param(&container_fx, "parallel", value)
            .map_err(DawError::operation_failed)?;
        Ok(())
    }

    fn container_channel_config(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        container_id: FxNodeId,
    ) -> Option<FxContainerChannelConfig> {
        let proj = resolve_project(&project)?;
        let (_track, fx_chain) = resolve_fx_chain(&proj, &chain)?;
        let raw_index = resolve_node_to_raw_index(&fx_chain, &container_id)?;
        let container_fx = fx_chain.fx_by_index_untracked(raw_index);
        Some(FxContainerChannelConfig {
            nch: read_config_u32(&container_fx, "container_nch"),
            nch_in: read_config_u32(&container_fx, "container_nch_in"),
            nch_out: read_config_u32(&container_fx, "container_nch_out"),
        })
    }

    fn set_container_channel_config(
        &self,
        project: ProjectContext,
        request: SetContainerChannelConfigRequest,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, chain) = resolve_fx_chain(&proj, &request.context).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", request.context))
        })?;
        let raw_index =
            resolve_node_to_raw_index(&chain, &request.container_id).ok_or_else(|| {
                DawError::operation_failed(format!(
                    "could not resolve container ({:?})",
                    request.container_id
                ))
            })?;
        let container_fx = chain.fx_by_index_untracked(raw_index);
        let params = [
            ("container_nch", request.config.nch),
            ("container_nch_in", request.config.nch_in),
            ("container_nch_out", request.config.nch_out),
        ];
        for (key, value) in &params {
            fx_sw::fx_set_named_config_param(&container_fx, key, &value.to_string())
                .map_err(DawError::operation_failed)?;
        }
        Ok(())
    }

    fn enclose_in_container(
        &self,
        project: ProjectContext,
        request: EncloseInContainerRequest,
    ) -> Option<FxNodeId> {
        let proj = resolve_project(&project)?;
        let (_track, chain) = resolve_fx_chain(&proj, &request.context)?;
        let track = chain.track()?;

        if request.node_ids.is_empty() {
            return None;
        }

        // Chunk-based approach: parse FXCHAIN with dawfile-reaper,
        // remove selected nodes, wrap them in a new container, serialize
        // back atomically. Safer than live stride-based API calls.
        let chunk = track
            .chunk(MAX_TRACK_CHUNK_SIZE, ChunkCacheHint::NormalMode)
            .ok()?;
        let chunk_str = chunk.to_string();

        let fxchain_start = chunk_str.find("<FXCHAIN")?;
        let fxchain_region = &chunk_str[fxchain_start..];
        let fxchain_end = find_block_end(fxchain_region)? + fxchain_start;
        let fxchain_text = &chunk_str[fxchain_start..=fxchain_end];

        let mut parsed_chain = dawfile_reaper::types::FxChain::parse(fxchain_text)
            .map_err(|e| warn!("Failed to parse FXCHAIN chunk: {}", e))
            .ok()?;

        info!(
            "enclose_in_container: parsed {} top-level nodes",
            parsed_chain.nodes.len()
        );

        let target_guids: std::collections::HashSet<String> = request
            .node_ids
            .iter()
            .filter(|id| !id.is_container())
            .map(|id| id.as_str().to_string())
            .collect();

        let mut matched_indices: Vec<usize> = Vec::new();
        for (i, node) in parsed_chain.nodes.iter().enumerate() {
            let fxid = match node {
                dawfile_reaper::types::FxChainNode::Plugin(p) => p.fxid.as_deref(),
                dawfile_reaper::types::FxChainNode::Container(c) => c.fxid.as_deref(),
            };
            if let Some(guid) = fxid {
                let guid_clean = guid
                    .trim_start_matches('{')
                    .trim_end_matches('}')
                    .to_lowercase();
                if target_guids.iter().any(|t| t.to_lowercase() == guid_clean) {
                    matched_indices.push(i);
                }
            }
        }

        if matched_indices.is_empty() {
            warn!(
                "enclose_in_container: no nodes matched target GUIDs: {:?}",
                target_guids
            );
            return None;
        }

        info!(
            "enclose_in_container: matched {} nodes at indices {:?}",
            matched_indices.len(),
            matched_indices
        );

        let insert_pos = matched_indices[0];
        let mut removed_nodes: Vec<dawfile_reaper::types::FxChainNode> = Vec::new();
        for &idx in matched_indices.iter().rev() {
            removed_nodes.push(parsed_chain.nodes.remove(idx));
        }
        removed_nodes.reverse();

        let container = dawfile_reaper::types::FxContainer {
            name: request.name.clone(),
            bypassed: false,
            offline: false,
            fxid: None,
            float_pos: None,
            parallel: false,
            container_cfg: Some([2, 2, 2, 0]),
            show: 0,
            last_sel: 0,
            docked: false,
            children: removed_nodes,
            raw_block: String::new(),
        };

        let insert_at = insert_pos.min(parsed_chain.nodes.len());
        parsed_chain.nodes.insert(
            insert_at,
            dawfile_reaper::types::FxChainNode::Container(container),
        );

        info!(
            "enclose_in_container: new chain has {} top-level nodes, container '{}' at index {}",
            parsed_chain.nodes.len(),
            request.name,
            insert_at
        );

        let new_fxchain_text = {
            use dawfile_reaper::RppSerialize;
            parsed_chain.to_rpp_string()
        };

        let mut new_chunk_str = String::with_capacity(chunk_str.len());
        new_chunk_str.push_str(&chunk_str[..fxchain_start]);
        new_chunk_str.push_str(&new_fxchain_text);
        if fxchain_end + 1 < chunk_str.len() {
            new_chunk_str.push_str(&chunk_str[fxchain_end + 1..]);
        }

        let new_chunk = reaper_high::Chunk::new(new_chunk_str);
        track.set_chunk(new_chunk).ok()?;

        info!("enclose_in_container: chunk set successfully");

        Some(FxNodeId::container(format!("{}", insert_at)))
    }

    fn explode_container(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        container_id: FxNodeId,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, fx_chain) = resolve_fx_chain(&proj, &chain).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", chain))
        })?;
        let track = fx_chain
            .track()
            .ok_or_else(|| DawError::operation_failed("track not found"))?;

        let chunk = track
            .chunk(MAX_TRACK_CHUNK_SIZE, ChunkCacheHint::NormalMode)
            .map_err(|e| DawError::operation_failed(format!("get chunk: {:?}", e)))?;
        let chunk_str = chunk.to_string();

        let fxchain_start = chunk_str
            .find("<FXCHAIN")
            .ok_or_else(|| DawError::operation_failed("no FXCHAIN block in track chunk"))?;
        let fxchain_region = &chunk_str[fxchain_start..];
        let fxchain_end = find_block_end(fxchain_region)
            .ok_or_else(|| DawError::operation_failed("could not find FXCHAIN closing tag"))?
            + fxchain_start;
        let fxchain_text = &chunk_str[fxchain_start..=fxchain_end];

        let mut parsed_chain = dawfile_reaper::types::FxChain::parse(fxchain_text)
            .map_err(|e| DawError::operation_failed(format!("parse FXCHAIN: {}", e)))?;

        let target_guid = if container_id.is_container() {
            let path_str = container_id
                .as_str()
                .strip_prefix("container:")
                .ok_or_else(|| DawError::operation_failed("invalid container id"))?;
            let idx: usize = path_str
                .split(':')
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| {
                    DawError::operation_failed(format!("invalid container path: {}", path_str))
                })?;
            parsed_chain.nodes.get(idx).and_then(|n| match n {
                dawfile_reaper::types::FxChainNode::Container(c) => c.fxid.clone(),
                _ => None,
            })
        } else {
            Some(container_id.as_str().to_string())
        };

        let container_idx = parsed_chain
            .nodes
            .iter()
            .position(|node| {
                if let dawfile_reaper::types::FxChainNode::Container(c) = node {
                    if let Some(ref target) = target_guid
                        && let Some(ref fxid) = c.fxid
                    {
                        let fxid_clean = fxid
                            .trim_start_matches('{')
                            .trim_end_matches('}')
                            .to_lowercase();
                        let target_clean = target
                            .trim_start_matches('{')
                            .trim_end_matches('}')
                            .to_lowercase();
                        return fxid_clean == target_clean;
                    }
                    false
                } else {
                    false
                }
            })
            .or_else(|| {
                if container_id.is_container() {
                    container_id
                        .as_str()
                        .strip_prefix("container:")
                        .and_then(|s| s.split(':').next())
                        .and_then(|s| s.parse::<usize>().ok())
                        .filter(|&idx| {
                            idx < parsed_chain.nodes.len()
                                && matches!(
                                    parsed_chain.nodes[idx],
                                    dawfile_reaper::types::FxChainNode::Container(_)
                                )
                        })
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                DawError::operation_failed(format!(
                    "could not find container {:?} in parsed FX chain",
                    container_id
                ))
            })?;

        let container_node = parsed_chain.nodes.remove(container_idx);
        if let dawfile_reaper::types::FxChainNode::Container(c) = container_node {
            info!(
                "explode_container: exploding '{}' with {} children at index {}",
                c.name,
                c.children.len(),
                container_idx
            );
            for (i, child) in c.children.into_iter().enumerate() {
                parsed_chain.nodes.insert(container_idx + i, child);
            }
        }

        let new_fxchain_text = {
            use dawfile_reaper::RppSerialize;
            parsed_chain.to_rpp_string()
        };
        let mut new_chunk_str = String::with_capacity(chunk_str.len());
        new_chunk_str.push_str(&chunk_str[..fxchain_start]);
        new_chunk_str.push_str(&new_fxchain_text);
        if fxchain_end + 1 < chunk_str.len() {
            new_chunk_str.push_str(&chunk_str[fxchain_end + 1..]);
        }

        let new_chunk = reaper_high::Chunk::new(new_chunk_str);
        track
            .set_chunk(new_chunk)
            .map_err(|e| DawError::operation_failed(format!("set chunk: {:?}", e)))?;

        info!("explode_container: chunk set successfully");
        Ok(())
    }

    fn rename_container(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        container_id: FxNodeId,
        name: &str,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (_track, fx_chain) = resolve_fx_chain(&proj, &chain).ok_or_else(|| {
            DawError::operation_failed(format!("FX chain not found ({:?})", chain))
        })?;
        let raw_index = resolve_node_to_raw_index(&fx_chain, &container_id).ok_or_else(|| {
            DawError::operation_failed(format!("could not resolve container ({:?})", container_id))
        })?;
        let container_fx = fx_chain.fx_by_index_untracked(raw_index);
        fx_sw::fx_set_named_config_param(&container_fx, "renamed_name", name)
            .map_err(DawError::operation_failed)?;
        Ok(())
    }

    // ── Raw chunk text ─────────────────────────────────────────────────

    fn chain_chunk_text(&self, project: ProjectContext, chain: FxChainContext) -> Option<String> {
        let proj = resolve_project(&project)?;
        let (track, _chain) = resolve_fx_chain(&proj, &chain)?;
        // NormalMode is mandatory — UndoMode returns stale cached state
        // after enclose_in_container etc.
        let chunk = track
            .chunk(MAX_TRACK_CHUNK_SIZE, ChunkCacheHint::NormalMode)
            .ok()?;
        let region = chunk.region();
        let fxchain_region = region.find_first_tag_named(0, "FXCHAIN")?;
        Some(fxchain_region.content().to_string())
    }

    fn insert_chain_chunk(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        chunk_text: &str,
    ) -> DawResult<()> {
        let proj = resolve_project(&project).ok_or_else(|| DawError::not_found("project", ""))?;
        let (track, _chain) = resolve_fx_chain(&proj, &chain)
            .ok_or_else(|| DawError::operation_failed("FX chain not found"))?;

        let mut chunk = track
            .chunk(MAX_TRACK_CHUNK_SIZE, ChunkCacheHint::NormalMode)
            .map_err(|e| DawError::operation_failed(format!("get chunk: {:?}", e)))?;
        let region = chunk.region();

        if let Some(fxchain_region) = region.find_first_tag_named(0, "FXCHAIN") {
            let closing_line = fxchain_region.last_line();
            chunk.insert_before_region_as_block(&closing_line, chunk_text);
        } else {
            let fxchain_block = format!("<FXCHAIN\nSHOW 0\nLASTSEL 0\nDOCKED 0\n{}\n>", chunk_text);
            let track_closing = chunk.region().last_line();
            chunk.insert_before_region_as_block(&track_closing, &fxchain_block);
        }

        track
            .set_chunk(chunk)
            .map_err(|e| DawError::operation_failed(format!("set chunk: {:?}", e)))?;
        info!("insert_chain_chunk: chunk set successfully");
        Ok(())
    }
}
