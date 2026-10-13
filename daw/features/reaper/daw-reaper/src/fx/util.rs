//! Shared FX helpers: project/chain resolution, FX builders, type
//! parsing, and named-config readers. Used by the trait impl and the
//! sibling submodules in `crate::fx`.

use daw_proto::{Fx, FxChainContext, FxNodeId, FxParameter, FxRef, FxType, ProjectContext};
use reaper_high::{FxChain, Reaper, Track};
use reaper_medium::TrackFxLocation;
use tracing::debug;

use crate::main_thread;
use crate::project_context::find_project_by_guid;

/// Resolve a [`ProjectContext`] to a REAPER [`reaper_high::Project`].
pub fn resolve_project(ctx: &ProjectContext) -> Option<reaper_high::Project> {
    match ctx {
        ProjectContext::Current => Some(Reaper::get().current_project()),
        ProjectContext::Project(guid) => find_project_by_guid(guid),
    }
}

/// Find a track by GUID within a project (without curly braces).
pub fn resolve_track_by_guid(project: &reaper_high::Project, guid: &str) -> Option<Track> {
    for i in 0..project.track_count() {
        if let Some(track) = project.track_by_index(i)
            && track.guid().to_string_without_braces() == guid
        {
            if !main_thread::is_track_valid(project, &track) {
                return None;
            }
            return Some(track);
        }
    }
    None
}

/// Get the FxChain for a given FxChainContext.
pub fn resolve_fx_chain(
    project: &reaper_high::Project,
    context: &FxChainContext,
) -> Option<(Track, FxChain)> {
    match context {
        FxChainContext::Track(guid) => {
            let track = resolve_track_by_guid(project, guid)?;
            let chain = track.normal_fx_chain();
            Some((track, chain))
        }
        FxChainContext::Input(guid) => {
            let track = resolve_track_by_guid(project, guid)?;
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

/// Resolve an [`FxRef`] (index/guid/name) to a raw chain index.
pub fn resolve_fx_index(chain: &FxChain, fx_ref: &FxRef) -> Option<u32> {
    match fx_ref {
        FxRef::Index(idx) => {
            if *idx < chain.fx_count() {
                Some(*idx)
            } else {
                None
            }
        }
        FxRef::Guid(guid) => {
            let node_id = FxNodeId::from_guid(guid.clone());
            super::tree::resolve_plugin_guid(chain, &node_id)
        }
        FxRef::Name(name) => {
            for fx in chain.index_based_fxs() {
                let fx_name = safe_fx_call("fx.name", None, || fx.name().to_str().to_string());
                if let Some(n) = fx_name
                    && n == *name
                {
                    return Some(fx.index());
                }
            }
            None
        }
    }
}

/// Convert chain-relative index to [`TrackFxLocation`] (input vs normal).
pub fn fx_location(index: u32, is_input: bool) -> TrackFxLocation {
    if is_input {
        TrackFxLocation::InputFxChain(index)
    } else {
        TrackFxLocation::NormalFxChain(index)
    }
}

/// Wrap a `reaper-high` FX accessor that may panic with stale-reference
/// `.expect()` calls. Returns `Some(value)` on success, `None` on panic
/// (logged via tracing). See issue #20.
pub fn safe_fx_call<T>(
    label: &'static str,
    fx_guid: Option<&str>,
    f: impl FnOnce() -> T,
) -> Option<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(value) => Some(value),
        Err(payload) => {
            let msg = panic_payload_message(&payload);
            tracing::warn!(
                label,
                fx_guid = fx_guid.unwrap_or(""),
                panic = %msg,
                "reaper-high FX accessor panicked; using fallback"
            );
            None
        }
    }
}

fn panic_payload_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "<non-string panic payload>".to_string()
}

/// REAPER FxInfo sub-type expression → our [`FxType`] enum.
pub fn parse_fx_type(sub_type: &str) -> FxType {
    match sub_type {
        "VST" | "VSTi" => FxType::Vst2,
        "VST3" | "VST3i" => FxType::Vst3,
        "AU" | "AUi" => FxType::Au,
        "JS" => FxType::Js,
        "CLAP" | "CLAPi" => FxType::Clap,
        _ => FxType::Unknown,
    }
}

/// Build an [`Fx`] proto struct from a `reaper-high` FX handle.
///
/// `chain` enables a fallback GUID lookup for container children whose
/// encoded indices (0x2000000+) can't be resolved by the bounds-checked
/// `fx_by_index()`.
pub fn build_fx_info(fx: &reaper_high::Fx, chain: Option<&FxChain>) -> Fx {
    let guid = fx
        .get_or_query_guid()
        .map(|g| g.to_string_without_braces())
        .unwrap_or_else(|_| {
            chain
                .and_then(|c| reaper_high::get_fx_guid(c, fx.index()))
                .map(|g| g.to_string_without_braces())
                .unwrap_or_default()
        });
    let name = safe_fx_call("fx.name", None, || fx.name().to_str().to_string())
        .unwrap_or_else(|| "(unknown)".to_string());
    let index = fx.index();
    let enabled = safe_fx_call("fx.is_enabled", None, || fx.is_enabled()).unwrap_or(false);
    let offline = safe_fx_call("fx.unknown", None, || !fx.is_online()).unwrap_or(false);
    let window_open =
        safe_fx_call("fx.window_is_open", None, || fx.window_is_open()).unwrap_or(false);
    let parameter_count =
        safe_fx_call("fx.parameter_count", None, || fx.parameter_count()).unwrap_or(0);

    let (plugin_name, plugin_type) = match fx.info() {
        Ok(info) => {
            let ptype = parse_fx_type(&info.sub_type_expression);
            (info.effect_name, ptype)
        }
        Err(_) => (name.clone(), FxType::Unknown),
    };

    let preset_name = safe_fx_call("fx.preset_name", None, || {
        fx.preset_name()
            .map(|rs| rs.to_str().to_string())
            .filter(|s| !s.is_empty())
    })
    .unwrap_or(None);

    Fx {
        guid,
        index,
        name,
        plugin_name,
        plugin_type,
        enabled,
        offline,
        window_open,
        parameter_count,
        preset_name,
    }
}

/// Build an [`FxParameter`] proto from a `reaper-high` `FxParameter`,
/// detecting discrete step counts via multiple heuristics.
pub fn build_fx_parameter(param: &reaper_high::FxParameter) -> FxParameter {
    let index = param.index();
    let name = param
        .name()
        .map(|n| n.to_str().to_string())
        .unwrap_or_default();
    let value = param.reaper_normalized_value().get();
    let formatted = param
        .formatted_value()
        .map(|f| f.to_str().to_string())
        .unwrap_or_else(|_| format!("{:.2}", value));

    let character = param.character();
    let is_toggle = matches!(character, reaper_high::FxParameterCharacter::Toggle);
    let step_sizes_result = param.step_sizes();

    debug!(
        "  param[{}] '{}': character={:?}, step_sizes={:?}, is_toggle={}",
        index, name, character, step_sizes_result, is_toggle
    );

    let is_discrete_character = matches!(character, reaper_high::FxParameterCharacter::Discrete);

    let step_count_from_sizes = step_sizes_result.and_then(|ss| {
        if let reaper_medium::GetParameterStepSizesResult::Normal { normal_step, .. } = ss {
            if normal_step > 0.0 {
                let n = (1.0 / normal_step).round() as u32;
                if (2..=256).contains(&n) {
                    Some(n)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    });

    let step_count = step_count_from_sizes.or_else(|| {
        if is_discrete_character {
            let mut count = 0u32;
            let mut last_label = String::new();
            for i in 0..=256 {
                let norm = (i as f64) / 256.0;
                if let Ok(s) = param.format_reaper_normalized_value(
                    reaper_medium::ReaperNormalizedFxParamValue::new(norm),
                ) {
                    let label = s.to_str().to_string();
                    if label != last_label {
                        count += 1;
                        last_label = label;
                    }
                }
            }
            if (2..=256).contains(&count) {
                debug!("    → discrete from character probe: {} steps", count);
                Some(count)
            } else {
                None
            }
        } else {
            None
        }
    });

    debug!("    → final step_count={:?}", step_count);

    let step_labels = if let Some(n) = step_count {
        let mut labels = Vec::with_capacity(n as usize);
        for i in 0..n {
            let norm = if n <= 1 {
                0.0
            } else {
                (i as f64) / ((n - 1) as f64)
            };
            let label = param
                .format_reaper_normalized_value(reaper_medium::ReaperNormalizedFxParamValue::new(
                    norm,
                ))
                .map(|s| s.to_str().to_string())
                .unwrap_or_else(|_| format!("{}", i));
            labels.push((norm, label));
        }
        labels
    } else {
        Vec::new()
    };

    FxParameter {
        index,
        name,
        value,
        formatted,
        is_toggle,
        step_count,
        step_labels,
    }
}

/// Read a named config param as a trimmed string. Strips embedded
/// NULs (raw `Vec<u8>` payload may include them).
pub fn read_config_str(fx: &reaper_high::Fx, key: &str) -> Option<String> {
    fx.get_named_config_param(key, 256).ok().map(|bytes| {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).trim().to_string()
    })
}

/// Read a named config param as `u32` (0 on failure).
pub fn read_config_u32(fx: &reaper_high::Fx, key: &str) -> u32 {
    read_config_str(fx, key)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
}

/// Read a named config param as `i32` (0 on failure).
pub fn read_config_i32(fx: &reaper_high::Fx, key: &str) -> i32 {
    read_config_str(fx, key)
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0)
}

/// Read a named config param as `f64` (0.0 on failure).
pub fn read_config_f64(fx: &reaper_high::Fx, key: &str) -> f64 {
    read_config_str(fx, key)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}
