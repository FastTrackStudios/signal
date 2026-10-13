//! Structured operations shared by DAW CLI commands.

use crate::rpc::Daw;
use eyre::Result;
use serde_json::{Value, json};

use crate::cli::{
    flags_str, format_position, fx_type_str, pan_to_string, resolve_track, vol_to_db,
};

fn shape_name(shape: &'static facet::Shape) -> String {
    match shape.module_path {
        Some(module) => format!("{module}::{}", shape.type_identifier),
        None => shape.type_identifier.to_string(),
    }
}

fn service_descriptor_json(service: &'static vox::ServiceDescriptor) -> Value {
    json!({
        "service": service.service_name,
        "doc": service.doc,
        "methods": service.methods.iter().map(|method| json!({
            "id": method.id.0,
            "service": method.service_name,
            "method": method.method_name,
            "doc": method.doc,
            "args_shape": shape_name(method.args_shape),
            "return_shape": shape_name(method.return_shape),
            // Note: vox 0.10 removed per-method retry metadata (retry is no
            // longer a vox concept), so there is no `retry` block to emit.
            "args": method.args.iter().map(|arg| json!({
                "name": arg.name,
                "shape": shape_name(arg.shape),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn daw_service_descriptors() -> Vec<&'static vox::ServiceDescriptor> {
    use crate::service;
    vec![
        service::action_registry::descriptor(),
        service::audio_accessor::descriptor(),
        service::audio_engine::descriptor(),
        service::automation::descriptor(),
        service::batch::descriptor(),
        service::dock_host::descriptor(),
        service::ext_state::descriptor(),
        service::fx::descriptor(),
        service::health::descriptor(),
        service::input::descriptor(),
        service::item::descriptor(),
        service::live_midi::descriptor(),
        service::marker::descriptor(),
        service::midi::descriptor(),
        service::peak::descriptor(),
        service::plugin_loader::descriptor(),
        service::position_conversion::descriptor(),
        service::project::descriptor(),
        service::region::descriptor(),
        service::resource::descriptor(),
        service::routing::descriptor(),
        service::screenset::descriptor(),
        service::take::descriptor(),
        service::tempo_map::descriptor(),
        service::toolbar::descriptor(),
        service::track::descriptor(),
        service::transport::descriptor(),
        service::dawfile_service::descriptor(),
    ]
}

pub fn service_catalog() -> Value {
    Value::Array(
        daw_service_descriptors()
            .into_iter()
            .map(service_descriptor_json)
            .collect(),
    )
}

fn fx_param_json(p: &crate::service::FxParameter) -> Value {
    let mut obj = json!({
        "index": p.index,
        "name": p.name,
        "value": p.value,
        "formatted": p.formatted,
        "is_toggle": p.is_toggle,
    });
    if let Some(steps) = p.step_count {
        obj["step_count"] = json!(steps);
    }
    if !p.step_labels.is_empty() {
        obj["step_labels"] = json!(
            p.step_labels
                .iter()
                .map(|(value, label)| json!({ "value": value, "label": label }))
                .collect::<Vec<_>>()
        );
    }
    obj
}

pub async fn project_info(daw: &Daw) -> Result<Value> {
    let project = daw.current_project().await?;
    let info = project.info().await?;
    let track_count = project.n_tracks().await?;
    let transport = project.transport().get_state().await?;

    Ok(json!({
        "name": info.name,
        "path": info.path,
        "guid": info.guid,
        "track_count": track_count,
        "tempo": transport.tempo.bpm,
        "time_signature": {
            "numerator": transport.time_signature.numerator,
            "denominator": transport.time_signature.denominator,
        },
    }))
}

pub async fn tracks(daw: &Daw) -> Result<Value> {
    let project = daw.current_project().await?;
    let all_tracks = project.tracks().all().await?;
    Ok(Value::Array(
        all_tracks
            .iter()
            .map(|t| {
                json!({
                    "index": t.index,
                    "name": t.name,
                    "guid": t.guid,
                    "muted": t.muted,
                    "soloed": t.soloed,
                    "armed": t.armed,
                    "flags": flags_str(t.muted, t.soloed, t.armed),
                    "selected": t.selected,
                    "volume": t.volume,
                    "volume_db": vol_to_db(t.volume),
                    "pan": t.pan,
                    "pan_display": pan_to_string(t.pan),
                    "is_folder": t.is_folder,
                    "folder_depth": t.folder_depth,
                    "fx_count": t.fx_count,
                    "input_fx_count": t.input_fx_count,
                })
            })
            .collect(),
    ))
}

pub async fn track(daw: &Daw, track_arg: &str) -> Result<Value> {
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    let t = handle.info().await?;
    Ok(json!({
        "index": t.index,
        "name": t.name,
        "guid": t.guid,
        "muted": t.muted,
        "soloed": t.soloed,
        "armed": t.armed,
        "flags": flags_str(t.muted, t.soloed, t.armed),
        "selected": t.selected,
        "volume": t.volume,
        "volume_db": vol_to_db(t.volume),
        "pan": t.pan,
        "pan_display": pan_to_string(t.pan),
        "is_folder": t.is_folder,
        "folder_depth": t.folder_depth,
        "parent_guid": t.parent_guid,
        "visible_in_tcp": t.visible_in_tcp,
        "visible_in_mixer": t.visible_in_mixer,
        "fx_count": t.fx_count,
        "input_fx_count": t.input_fx_count,
        "color": t.color,
    }))
}

pub async fn fx(daw: &Daw, track_arg: &str) -> Result<Value> {
    let (guid, track_name) = resolve_track(daw, track_arg).await?;
    let project = daw.current_project().await?;
    let handle = project
        .tracks()
        .by_guid(&guid)
        .await?
        .ok_or_else(|| eyre::eyre!("Track not found"))?;
    let fx_list = handle.fx_chain().all().await?;
    Ok(json!({
        "track": track_name,
        "track_guid": guid,
        "fx": fx_list.iter().map(|f| json!({
            "index": f.index,
            "name": f.name,
            "plugin_name": f.plugin_name,
            "plugin_type": fx_type_str(&f.plugin_type),
            "guid": f.guid,
            "enabled": f.enabled,
            "offline": f.offline,
            "parameter_count": f.parameter_count,
            "preset_name": f.preset_name,
        })).collect::<Vec<_>>(),
    }))
}

pub async fn plugins(daw: &Daw) -> Result<Value> {
    let plugins = daw.installed_plugins().await?;
    Ok(Value::Array(
        plugins
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "ident": p.ident,
                })
            })
            .collect(),
    ))
}

pub async fn last_touched_fx(daw: &Daw) -> Result<Value> {
    let touched = daw.last_touched_fx().await?;
    Ok(json!({
        "last_touched_fx": touched.as_ref().map(|fx| format!("{fx:?}")),
    }))
}

pub async fn fx_params(daw: &Daw, track_arg: &str, fx_arg: &str) -> Result<Value> {
    let (_, track_name) = resolve_track(daw, track_arg).await?;
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    let fx_chain = handle.fx_chain();
    let fx_handle = crate::cli::resolve_fx_handle(&fx_chain, fx_arg, &track_name).await?;
    let fx_info = fx_handle.info().await?;
    let params = fx_handle.parameters().await?;
    Ok(json!({
        "track": track_name,
        "fx": {
            "index": fx_info.index,
            "name": fx_info.name,
            "guid": fx_info.guid,
        },
        "parameters": params.iter().map(fx_param_json).collect::<Vec<_>>(),
    }))
}

pub async fn fx_set_param(
    daw: &Daw,
    track_arg: &str,
    fx_arg: &str,
    param: u32,
    value: f64,
) -> Result<Value> {
    let (_, track_name) = resolve_track(daw, track_arg).await?;
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    let fx_chain = handle.fx_chain();
    let fx_handle = crate::cli::resolve_fx_handle(&fx_chain, fx_arg, &track_name).await?;
    fx_handle.param(param).set(value).await?;
    let updated = fx_handle.param(param).info().await?;
    Ok(json!({
        "track": track_name,
        "fx_guid": fx_handle.guid(),
        "parameter": fx_param_json(&updated),
    }))
}

pub async fn fx_set_param_by_name(
    daw: &Daw,
    track_arg: &str,
    fx_arg: &str,
    param: &str,
    value: f64,
) -> Result<Value> {
    let (_, track_name) = resolve_track(daw, track_arg).await?;
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    let fx_chain = handle.fx_chain();
    let fx_handle = crate::cli::resolve_fx_handle(&fx_chain, fx_arg, &track_name).await?;
    fx_handle.param_by_name(param).set(value).await?;
    let updated = fx_handle.param_by_name(param).info().await?;
    Ok(json!({
        "track": track_name,
        "fx_guid": fx_handle.guid(),
        "parameter": fx_param_json(&updated),
    }))
}

pub async fn fx_add(
    daw: &Daw,
    track_arg: &str,
    fx_name: &str,
    at_index: Option<u32>,
) -> Result<Value> {
    let (_, track_name) = resolve_track(daw, track_arg).await?;
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    let fx_handle = match at_index {
        Some(index) => handle.fx_chain().add_at(fx_name, index).await?,
        None => handle.fx_chain().add(fx_name).await?,
    };
    let info = fx_handle.info().await?;
    Ok(json!({
        "track": track_name,
        "fx": {
            "index": info.index,
            "name": info.name,
            "plugin_name": info.plugin_name,
            "guid": info.guid,
            "enabled": info.enabled,
        }
    }))
}

pub async fn fx_remove(daw: &Daw, track_arg: &str, fx_arg: &str) -> Result<Value> {
    let (_, track_name) = resolve_track(daw, track_arg).await?;
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    let fx_chain = handle.fx_chain();
    let fx_handle = crate::cli::resolve_fx_handle(&fx_chain, fx_arg, &track_name).await?;
    let info = fx_handle.info().await?;
    fx_handle.remove().await?;
    Ok(json!({
        "removed": true,
        "track": track_name,
        "fx": {
            "index": info.index,
            "name": info.name,
            "guid": info.guid,
        }
    }))
}

pub async fn fx_set_enabled(
    daw: &Daw,
    track_arg: &str,
    fx_arg: &str,
    enabled: bool,
) -> Result<Value> {
    let (_, track_name) = resolve_track(daw, track_arg).await?;
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    let fx_chain = handle.fx_chain();
    let fx_handle = crate::cli::resolve_fx_handle(&fx_chain, fx_arg, &track_name).await?;
    if enabled {
        fx_handle.enable().await?;
    } else {
        fx_handle.disable().await?;
    }
    let info = fx_handle.info().await?;
    Ok(json!({
        "track": track_name,
        "fx_guid": info.guid,
        "enabled": info.enabled,
    }))
}

pub async fn fx_move(daw: &Daw, track_arg: &str, fx_arg: &str, index: u32) -> Result<Value> {
    let (_, track_name) = resolve_track(daw, track_arg).await?;
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    let fx_chain = handle.fx_chain();
    let fx_handle = crate::cli::resolve_fx_handle(&fx_chain, fx_arg, &track_name).await?;
    fx_handle.move_to(index).await?;
    let info = fx_handle.info().await?;
    Ok(json!({
        "track": track_name,
        "fx_guid": info.guid,
        "index": info.index,
    }))
}

pub async fn fx_ui(daw: &Daw, track_arg: &str, fx_arg: &str, action: &str) -> Result<Value> {
    let (_, track_name) = resolve_track(daw, track_arg).await?;
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    let fx_chain = handle.fx_chain();
    let fx_handle = crate::cli::resolve_fx_handle(&fx_chain, fx_arg, &track_name).await?;
    match action {
        "open" => fx_handle.open_ui().await?,
        "close" => fx_handle.close_ui().await?,
        "toggle" => fx_handle.toggle_ui().await?,
        _ => eyre::bail!("unknown FX UI action: {action}"),
    }
    let info = fx_handle.info().await?;
    Ok(json!({
        "track": track_name,
        "fx_guid": info.guid,
        "window_open": info.window_open,
    }))
}

pub async fn fx_preset(
    daw: &Daw,
    track_arg: &str,
    fx_arg: &str,
    action: &str,
    index: Option<u32>,
) -> Result<Value> {
    let (_, track_name) = resolve_track(daw, track_arg).await?;
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    let fx_chain = handle.fx_chain();
    let fx_handle = crate::cli::resolve_fx_handle(&fx_chain, fx_arg, &track_name).await?;
    match action {
        "get" => {}
        "next" => fx_handle.next_preset().await?,
        "previous" | "prev" => fx_handle.prev_preset().await?,
        "set" => {
            fx_handle
                .set_preset(index.ok_or_else(|| eyre::eyre!("preset index is required"))?)
                .await?
        }
        _ => eyre::bail!("unknown preset action: {action}"),
    }
    Ok(json!({
        "track": track_name,
        "fx_guid": fx_handle.guid(),
        "preset": fx_handle.preset_index().await?.map(|preset| format!("{preset:?}")),
    }))
}

pub async fn transport(daw: &Daw) -> Result<Value> {
    let project = daw.current_project().await?;
    let state = project.transport().get_state().await?;
    Ok(json!({
        "play_state": format!("{:?}", state.play_state),
        "record_mode": format!("{:?}", state.record_mode),
        "looping": state.looping,
        "tempo": state.tempo.bpm,
        "playrate": state.playrate,
        "time_signature": {
            "numerator": state.time_signature.numerator,
            "denominator": state.time_signature.denominator,
        },
        "playhead": format_position(&state.playhead_position),
        "edit_cursor": format_position(&state.edit_position),
        "loop_region": state.loop_region.as_ref().map(|lr| json!({
            "start_seconds": lr.start_seconds,
            "end_seconds": lr.end_seconds,
        })),
    }))
}

pub async fn transport_control(daw: &Daw, action: &str) -> Result<Value> {
    let project = daw.current_project().await?;
    let transport = project.transport();
    match action {
        "play" => transport.play().await?,
        "pause" => transport.pause().await?,
        "stop" => transport.stop().await?,
        "play_pause" => transport.play_pause().await?,
        "play_stop" => transport.play_stop().await?,
        "record" => transport.record().await?,
        "stop_recording" => transport.stop_recording().await?,
        "toggle_recording" => transport.toggle_recording().await?,
        "goto_start" => transport.goto_start().await?,
        "goto_end" => transport.goto_end().await?,
        "toggle_loop" => transport.toggle_loop().await?,
        _ => eyre::bail!("unknown transport action: {action}"),
    }
    transport_state_for_project(&project).await
}

async fn transport_state_for_project(project: &crate::rpc::Project) -> Result<Value> {
    let state = project.transport().get_state().await?;
    Ok(json!({
        "play_state": format!("{:?}", state.play_state),
        "record_mode": format!("{:?}", state.record_mode),
        "looping": state.looping,
        "tempo": state.tempo.bpm,
        "playrate": state.playrate,
        "time_signature": {
            "numerator": state.time_signature.numerator,
            "denominator": state.time_signature.denominator,
        },
        "playhead": format_position(&state.playhead_position),
        "edit_cursor": format_position(&state.edit_position),
    }))
}

pub async fn markers(daw: &Daw) -> Result<Value> {
    let project = daw.current_project().await?;
    let markers = project.markers().all().await?;
    Ok(Value::Array(
        markers
            .iter()
            .map(|m| {
                json!({
                    "id": m.id,
                    "name": m.name,
                    "position": format_position(&m.position),
                    "color": m.color,
                    "guid": m.guid,
                })
            })
            .collect(),
    ))
}

pub async fn regions(daw: &Daw) -> Result<Value> {
    let project = daw.current_project().await?;
    let regions = project.regions().all().await?;
    Ok(Value::Array(
        regions
            .iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "name": r.name,
                    "start": format_position(&r.time_range.start),
                    "end": format_position(&r.time_range.end),
                    "color": r.color,
                    "guid": r.guid,
                })
            })
            .collect(),
    ))
}

pub async fn projects(daw: &Daw) -> Result<Value> {
    let projects = daw.projects().await?;
    let mut arr = Vec::new();
    for (i, p) in projects.iter().enumerate() {
        let info = p.info().await?;
        arr.push(json!({
            "index": i,
            "name": info.name,
            "guid": info.guid,
            "path": info.path,
        }));
    }
    Ok(Value::Array(arr))
}

pub async fn create_project(daw: &Daw) -> Result<Value> {
    let project = daw.create_project().await?;
    let info = project.info().await?;
    Ok(json!({ "name": info.name, "guid": info.guid, "path": info.path }))
}

pub async fn select_project(daw: &Daw, guid: &str) -> Result<Value> {
    let project = daw.select_project(guid).await?;
    let info = project.info().await?;
    Ok(json!({ "selected": true, "name": info.name, "guid": info.guid, "path": info.path }))
}

pub async fn open_project(daw: &Daw, path: &str) -> Result<Value> {
    let project = daw.open_project(path).await?;
    let info = project.info().await?;
    Ok(json!({
        "name": info.name,
        "guid": info.guid,
        "path": info.path,
    }))
}

pub async fn close_project(daw: &Daw, guid: Option<&str>) -> Result<Value> {
    let target_guid = match guid {
        Some(guid) => guid.to_string(),
        None => daw.current_project().await?.info().await?.guid,
    };
    daw.close_project(&target_guid).await?;
    Ok(json!({ "closed": true, "guid": target_guid }))
}

pub async fn save_project(daw: &Daw) -> Result<Value> {
    let project = daw.current_project().await?;
    let info = project.info().await?;
    project.save().await?;
    Ok(json!({ "saved": true, "guid": info.guid, "path": info.path }))
}

pub async fn save_all_projects(daw: &Daw) -> Result<Value> {
    daw.save_all_projects().await?;
    Ok(json!({ "saved_all": true }))
}

pub async fn project_undo(daw: &Daw) -> Result<Value> {
    let project = daw.current_project().await?;
    Ok(json!({ "undone": project.undo().await? }))
}

pub async fn project_redo(daw: &Daw) -> Result<Value> {
    let project = daw.current_project().await?;
    Ok(json!({ "redone": project.redo().await? }))
}

pub async fn project_run_command(daw: &Daw, command: &str) -> Result<Value> {
    let project = daw.current_project().await?;
    Ok(json!({ "command": command, "executed": project.run_command(command).await? }))
}

pub async fn add_track(daw: &Daw, name: Option<&str>, at_index: Option<u32>) -> Result<Value> {
    let project = daw.current_project().await?;
    let handle = project
        .tracks()
        .add(name.unwrap_or("New Track"), at_index)
        .await?;
    let info = handle.info().await?;
    Ok(json!({
        "index": info.index,
        "name": info.name,
        "guid": info.guid,
    }))
}

pub async fn track_set(daw: &Daw, track_arg: &str, field: &str, value: Value) -> Result<Value> {
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    match field {
        "muted" => {
            if value
                .as_bool()
                .ok_or_else(|| eyre::eyre!("muted expects bool"))?
            {
                handle.mute().await?
            } else {
                handle.unmute().await?
            }
        }
        "soloed" => {
            if value
                .as_bool()
                .ok_or_else(|| eyre::eyre!("soloed expects bool"))?
            {
                handle.solo().await?
            } else {
                handle.unsolo().await?
            }
        }
        "armed" => {
            if value
                .as_bool()
                .ok_or_else(|| eyre::eyre!("armed expects bool"))?
            {
                handle.arm().await?
            } else {
                handle.disarm().await?
            }
        }
        "selected" => {
            if value
                .as_bool()
                .ok_or_else(|| eyre::eyre!("selected expects bool"))?
            {
                handle.select().await?
            } else {
                handle.deselect().await?
            }
        }
        "volume" => {
            handle
                .set_volume(
                    value
                        .as_f64()
                        .ok_or_else(|| eyre::eyre!("volume expects number"))?,
                )
                .await?
        }
        "pan" => {
            handle
                .set_pan(
                    value
                        .as_f64()
                        .ok_or_else(|| eyre::eyre!("pan expects number"))?,
                )
                .await?
        }
        "name" => {
            handle
                .rename(
                    value
                        .as_str()
                        .ok_or_else(|| eyre::eyre!("name expects string"))?,
                )
                .await?
        }
        "color" => {
            handle
                .set_color(
                    value
                        .as_u64()
                        .ok_or_else(|| eyre::eyre!("color expects integer"))?
                        as u32,
                )
                .await?
        }
        "folder_depth" | "num_channels" | "visible_in_tcp" | "visible_in_mixer" => {
            // Retired alongside the architect::rpc port — these
            // properties live on sibling traits when revived.
            return Err(eyre::eyre!(
                "track property '{field}' retired with the architect::rpc port"
            ));
        }
        "parent_send" => {
            handle
                .set_parent_send(
                    value
                        .as_bool()
                        .ok_or_else(|| eyre::eyre!("parent_send expects bool"))?,
                )
                .await?
        }
        _ => eyre::bail!("unsupported track field: {field}"),
    }
    let info = handle.info().await?;
    Ok(json!({
        "index": info.index,
        "name": info.name,
        "guid": info.guid,
        "muted": info.muted,
        "soloed": info.soloed,
        "armed": info.armed,
        "selected": info.selected,
        "volume": info.volume,
        "pan": info.pan,
        "color": info.color,
        "visible_in_tcp": info.visible_in_tcp,
        "visible_in_mixer": info.visible_in_mixer,
    }))
}

pub async fn track_rename(daw: &Daw, track_arg: &str, name: &str) -> Result<Value> {
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    handle.rename(name).await?;
    let info = handle.info().await?;
    Ok(json!({ "guid": info.guid, "index": info.index, "name": info.name }))
}

pub async fn track_set_color(daw: &Daw, track_arg: &str, color: u32) -> Result<Value> {
    let handle = crate::cli::resolve_track_handle(daw, track_arg).await?;
    handle.set_color(color).await?;
    let info = handle.info().await?;
    Ok(json!({
        "guid": info.guid,
        "index": info.index,
        "name": info.name,
        "color": info.color,
    }))
}

// Retired alongside the architect::rpc port: folder depth, move,
// per-track ext state. These returned `not supported` errors below
// rather than disappearing from the CLI surface so the
// subcommand-driving code keeps compiling — wire them to a real impl
// once sibling traits land.

pub async fn track_set_folder_depth(_daw: &Daw, _track_arg: &str, _depth: i32) -> Result<Value> {
    Err(eyre::eyre!(
        "track set-folder-depth retired with the architect::rpc port"
    ))
}

pub async fn track_move(_daw: &Daw, _track_arg: &str, _index: u32) -> Result<Value> {
    Err(eyre::eyre!(
        "track move retired with the architect::rpc port"
    ))
}

pub async fn track_ext_state(
    _daw: &Daw,
    _track_arg: &str,
    _section: &str,
    _key: &str,
    _value: Option<&str>,
) -> Result<Value> {
    Err(eyre::eyre!(
        "track ext-state retired with the architect::rpc port"
    ))
}

pub async fn track_delete_ext_state(
    _daw: &Daw,
    _track_arg: &str,
    _section: &str,
    _key: &str,
) -> Result<Value> {
    Err(eyre::eyre!(
        "track delete-ext-state retired with the architect::rpc port"
    ))
}

pub async fn remove_track(daw: &Daw, track_arg: &str) -> Result<Value> {
    let (guid, name) = resolve_track(daw, track_arg).await?;
    let project = daw.current_project().await?;
    project
        .tracks()
        .remove(crate::service::TrackRef::Guid(guid.clone()))
        .await?;
    Ok(json!({
        "removed": true,
        "name": name,
        "guid": guid,
    }))
}

pub async fn audio_engine(daw: &Daw) -> Result<Value> {
    let engine = daw.audio_engine();
    Ok(json!({
        "state": format!("{:?}", engine.get_state().await?),
        "latency": format!("{:?}", engine.get_latency().await?),
        "output_latency_seconds": engine.output_latency_seconds().await?,
        "is_running": engine.is_running().await?,
        "inputs": format!("{:?}", engine.get_audio_inputs().await?),
    }))
}

pub async fn audio_engine_control(daw: &Daw, action: &str) -> Result<Value> {
    let engine = daw.audio_engine();
    match action {
        "init" => engine.init().await?,
        "quit" => engine.quit().await?,
        _ => eyre::bail!("unknown audio engine action: {action}"),
    }
    Ok(json!({ "action": action, "is_running": engine.is_running().await? }))
}

pub async fn plugin_loader_load(daw: &Daw, path: &str) -> Result<Value> {
    Ok(
        json!({ "path": path, "result": format!("{:?}", daw.plugin_loader().load_plugin(path).await?) }),
    )
}

pub async fn plugin_loader_list(daw: &Daw) -> Result<Value> {
    Ok(json!({ "loaded": format!("{:?}", daw.plugin_loader().list_loaded().await?) }))
}

pub async fn action_execute(daw: &Daw, action_id: &str) -> Result<Value> {
    let result = daw
        .action_registry()
        .execute_action_detailed(action_id)
        .await?;
    Ok(json!({
        "requested_action": result.requested_action,
        "executed": result.executed,
        "command_id": result.command_id,
        "command_name": result.command_name,
        "description": result.description,
        "origin": result.origin.map(|origin| format!("{origin:?}")),
        "provider": result.provider,
        "provider_tags": result.provider_tags,
        "registered_by_fts": result.registered_by_fts,
        "toggle_state_before": result.toggle_state_before,
        "toggle_state_after": result.toggle_state_after,
    }))
}

struct ActionAlias {
    alias: &'static str,
    action_id: &'static str,
    provider: &'static str,
    description: &'static str,
}

const ACTION_ALIASES: &[ActionAlias] = &[
    ActionAlias {
        alias: "transport.play",
        action_id: "1007",
        provider: "reaper",
        description: "Transport: Play",
    },
    ActionAlias {
        alias: "transport.stop",
        action_id: "1016",
        provider: "reaper",
        description: "Transport: Stop",
    },
    ActionAlias {
        alias: "transport.pause",
        action_id: "1008",
        provider: "reaper",
        description: "Transport: Pause",
    },
    ActionAlias {
        alias: "transport.record",
        action_id: "1013",
        provider: "reaper",
        description: "Transport: Record",
    },
    ActionAlias {
        alias: "transport.play_stop",
        action_id: "40044",
        provider: "reaper",
        description: "Transport: Play/stop",
    },
    ActionAlias {
        alias: "edit.undo",
        action_id: "40029",
        provider: "reaper",
        description: "Edit: Undo",
    },
    ActionAlias {
        alias: "edit.redo",
        action_id: "40030",
        provider: "reaper",
        description: "Edit: Redo",
    },
    ActionAlias {
        alias: "marker.insert",
        action_id: "40157",
        provider: "reaper",
        description: "Markers: Insert marker at current position",
    },
    ActionAlias {
        alias: "region.insert",
        action_id: "40306",
        provider: "reaper",
        description: "Regions: Insert region from time selection",
    },
];

fn action_alias(alias: &str) -> Option<&'static ActionAlias> {
    ACTION_ALIASES
        .iter()
        .find(|entry| entry.alias.eq_ignore_ascii_case(alias))
}

pub fn action_aliases() -> Value {
    json!({
        "count": ACTION_ALIASES.len(),
        "aliases": ACTION_ALIASES.iter().map(|entry| json!({
            "alias": entry.alias,
            "action_id": entry.action_id,
            "provider": entry.provider,
            "description": entry.description,
        })).collect::<Vec<_>>(),
    })
}

pub async fn action_execute_alias(daw: &Daw, alias: &str) -> Result<Value> {
    let entry = action_alias(alias).ok_or_else(|| eyre::eyre!("unknown action alias: {alias}"))?;
    let mut value = action_execute(daw, entry.action_id).await?;
    value["alias"] = json!(entry.alias);
    value["alias_description"] = json!(entry.description);
    value["alias_provider"] = json!(entry.provider);
    Ok(value)
}

pub async fn action_lookup(daw: &Daw, command_name: &str) -> Result<Value> {
    let registry = daw.action_registry();
    Ok(json!({
        "command_name": command_name,
        "registered": registry.is_registered(command_name).await?,
        "in_action_list": registry.is_in_action_list(command_name).await?,
        "command_id": registry.lookup_command_id(command_name).await?,
        "toggle_state": registry.get_toggle_state(command_name).await?,
    }))
}

pub async fn action_list(
    daw: &Daw,
    filter: &str,
    section: &str,
    query: Option<&str>,
    limit: Option<u32>,
) -> Result<Value> {
    let filter = match filter.trim().to_ascii_lowercase().as_str() {
        "all" => crate::service::ActionListFilter::All,
        "reaper" | "native" | "built-in" | "builtin" => crate::service::ActionListFilter::Reaper,
        "non-reaper" | "nonreaper" | "extension" | "extensions" | "custom" => {
            crate::service::ActionListFilter::NonReaper
        }
        "sws" | "sws/s&m" | "s&m" => crate::service::ActionListFilter::Sws,
        "fts" | "fasttrackstudio" => crate::service::ActionListFilter::Fts,
        "registered" | "local" => crate::service::ActionListFilter::Registered,
        _ => eyre::bail!("action filter must be all, reaper, non-reaper, sws, fts, or registered"),
    };
    let section = parse_action_section(section)?;

    let request = crate::service::ActionListRequest {
        filter,
        section,
        query: query.map(str::to_string),
        limit,
    };
    let response = daw.action_registry().list_actions(request).await?;
    Ok(json!({
        "filter": format!("{filter:?}"),
        "section": {
            "id": section.unique_id(),
            "name": section.name(),
        },
        "query": query,
        "count": response.actions.len(),
        "total_count": response.total_count,
        "limited": limit.is_some_and(|limit| response.total_count > limit),
        "actions": response.actions.iter().map(|action| json!({
            "command_id": action.command_id,
            "section_id": action.section_id,
            "section_name": action.section_name,
            "command_name": action.command_name,
            "description": action.description,
            "origin": format!("{:?}", action.origin),
            "provider": action.provider,
            "provider_tags": action.provider_tags,
            "registered_by_fts": action.registered_by_fts,
            "toggle_state": action.toggle_state,
        })).collect::<Vec<_>>(),
    }))
}

fn parse_action_section(section: &str) -> Result<crate::service::ActionSection> {
    match section.trim().to_ascii_lowercase().as_str() {
        "main" | "0" => Ok(crate::service::ActionSection::Main),
        "main-alt" | "main_alt" | "100" => Ok(crate::service::ActionSection::MainAlt),
        "midi-editor" | "midi_editor" | "midi" | "32060" => {
            Ok(crate::service::ActionSection::MidiEditor)
        }
        "midi-event-list-editor" | "midi_event_list_editor" | "midi-event-list" | "32061" => {
            Ok(crate::service::ActionSection::MidiEventListEditor)
        }
        "midi-inline-editor" | "midi_inline_editor" | "midi-inline" | "32062" => {
            Ok(crate::service::ActionSection::MidiInlineEditor)
        }
        "media-explorer" | "media_explorer" | "explorer" | "32063" => {
            Ok(crate::service::ActionSection::MediaExplorer)
        }
        raw => raw
            .parse::<u32>()
            .map(crate::service::ActionSection::Custom)
            .map_err(|_| {
                eyre::eyre!(
                    "action section must be main, main-alt, midi-editor, midi-event-list-editor, midi-inline-editor, media-explorer, or a numeric section ID"
                )
            }),
    }
}

pub async fn action_set_toggle(daw: &Daw, command_name: &str, is_on: bool) -> Result<Value> {
    daw.action_registry()
        .set_toggle_state(command_name, is_on)
        .await?;
    Ok(
        json!({ "command_name": command_name, "toggle_state": daw.action_registry().get_toggle_state(command_name).await? }),
    )
}

/// Reload REAPER's colour theme; `None` re-opens the active one.
///
/// The `loaded` flag is REAPER's own answer, not ours — a path it can't open
/// comes back false rather than erroring, so report it rather than pretending
/// the reload worked.
pub async fn theme_reload(daw: &Daw, path: Option<&std::path::Path>) -> Result<Value> {
    let resources = daw.resources();
    let loaded = match path {
        Some(p) => {
            // Resolve here: REAPER interprets a relative path against its own
            // working directory, not the shell's.
            let abs =
                std::fs::canonicalize(p).map_err(|e| eyre::eyre!("theme {}: {e}", p.display()))?;
            resources.load_color_theme(abs).await?
        }
        None => resources.reload_color_theme().await?,
    };
    Ok(json!({
        "loaded": loaded,
        "theme": resources.color_theme_path().await?
            .map(|p| p.display().to_string()),
    }))
}

/// REAPER's resource, ini and active-theme paths.
pub async fn theme_paths(daw: &Daw) -> Result<Value> {
    let resources = daw.resources();
    Ok(json!({
        "resource_path": resources.resource_path().await?.display().to_string(),
        "ini_file": resources.ini_file_path().await?.display().to_string(),
        "color_theme": resources.color_theme_path().await?
            .map(|p| p.display().to_string()),
    }))
}

pub async fn toolbar_status(daw: &Daw) -> Result<Value> {
    let toolbar = daw.toolbar();
    let tracked = toolbar.get_tracked_buttons().await?;
    Ok(json!({
        "available": toolbar.is_available().await?,
        "tracked_buttons": tracked.iter().map(|button| json!({
            "toolbar_name": button.toolbar_name,
            "command_name": button.command_name,
            "workflow_id": button.workflow_id,
        })).collect::<Vec<_>>(),
    }))
}

pub fn parse_toolbar_target(target: &str) -> Result<crate::service::ToolbarTarget> {
    let normalized = target.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "main" | "main-toolbar" | "main_toolbar" | "main toolbar" | "0" => {
            Ok(crate::service::ToolbarTarget::Main)
        }
        value => {
            if let Some(raw) = value
                .strip_prefix("midi toolbar ")
                .or_else(|| value.strip_prefix("midi-toolbar-"))
                .or_else(|| value.strip_prefix("midi_toolbar_"))
                .or_else(|| value.strip_prefix("midi "))
                .or_else(|| value.strip_prefix("midi-"))
                .or_else(|| value.strip_prefix("midi_"))
            {
                let number = raw.parse::<u8>().map_err(|_| {
                    eyre::eyre!(
                        "toolbar target must be main, floating toolbar 1-32, or MIDI toolbar 1-8"
                    )
                })?;
                if !(1..=8).contains(&number) {
                    eyre::bail!("MIDI toolbar number must be between 1 and 8");
                }
                return Ok(crate::service::ToolbarTarget::Midi(number));
            }

            let number = value
                .strip_prefix("floating toolbar ")
                .or_else(|| value.strip_prefix("floating-toolbar-"))
                .or_else(|| value.strip_prefix("floating_toolbar_"))
                .or_else(|| value.strip_prefix("floating "))
                .or_else(|| value.strip_prefix("floating-"))
                .or_else(|| value.strip_prefix("floating_"))
                .unwrap_or(value)
                .parse::<u8>()
                .map_err(|_| {
                    eyre::eyre!(
                        "toolbar target must be main, floating toolbar 1-32, or MIDI toolbar 1-8"
                    )
                })?;
            if !(1..=32).contains(&number) {
                eyre::bail!("floating toolbar number must be between 1 and 32");
            }
            Ok(crate::service::ToolbarTarget::Floating(number))
        }
    }
}

fn toolbar_snapshots_json(snapshots: &[crate::service::ToolbarSnapshot]) -> Value {
    let json = facet_json::to_string(snapshots).unwrap_or_else(|_| "[]".to_string());
    serde_json::from_str(&json).unwrap_or(Value::Array(Vec::new()))
}

pub async fn toolbar_live(daw: &Daw, target: Option<&str>) -> Result<Value> {
    let toolbar = daw.toolbar();
    let json = if let Some(target) = target {
        toolbar
            .get_live_toolbar_json(parse_toolbar_target(target)?)
            .await?
    } else {
        toolbar.get_live_toolbars_json().await?
    };
    Ok(serde_json::from_str(&json)?)
}

pub async fn toolbar_config(daw: &Daw, path: &str, target: Option<&str>) -> Result<Value> {
    let _ = daw;
    let mut snapshots = dawfile_reaper::toolbar_config::parse_toolbar_config_file(path)?;
    if let Some(target) = target {
        let target_name = match parse_toolbar_target(target)? {
            crate::service::ToolbarTarget::Main => "Main toolbar".to_string(),
            crate::service::ToolbarTarget::Floating(n) => format!("Floating toolbar {n}"),
            crate::service::ToolbarTarget::Midi(n) => format!("Floating MIDI toolbar {n}"),
        };
        snapshots.retain(|snapshot| snapshot.toolbar_name == target_name);
    }
    Ok(toolbar_snapshots_json(&snapshots))
}

fn toolbar_operation_json(result: crate::service::ToolbarResult) -> Result<Value> {
    if result.ok {
        Ok(json!({
            "ok": true,
            "command_id": result.command_id,
        }))
    } else {
        match result.error {
            Some(message) => Err(eyre::eyre!(message)),
            None => Err(eyre::eyre!("toolbar operation failed")),
        }
    }
}

fn make_toolbar_icon(
    icon: Option<&str>,
    kind: crate::service::ToolbarIconKind,
) -> Option<crate::service::ToolbarIcon> {
    icon.map(|value| crate::service::ToolbarIcon {
        kind,
        value: value.to_string(),
    })
}

fn toolbar_button(
    command_name: &str,
    label: &str,
    target: &str,
    position: Option<u32>,
    icon: Option<&str>,
    icon_kind: crate::service::ToolbarIconKind,
    flags: u32,
) -> Result<crate::service::ToolbarButton> {
    Ok(crate::service::ToolbarButton {
        command_name: command_name.to_string(),
        label: label.to_string(),
        icon: make_toolbar_icon(icon, icon_kind),
        target: parse_toolbar_target(target)?,
        placement: position
            .map(crate::service::ToolbarPlacement::Position)
            .unwrap_or(crate::service::ToolbarPlacement::Append),
        flags,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn toolbar_add(
    daw: &Daw,
    command_name: &str,
    label: &str,
    target: &str,
    workflow_id: &str,
    position: Option<u32>,
    icon: Option<&str>,
    icon_kind: crate::service::ToolbarIconKind,
    flags: u32,
) -> Result<Value> {
    let button = toolbar_button(
        command_name,
        label,
        target,
        position,
        icon,
        icon_kind,
        flags,
    )?;
    toolbar_operation_json(daw.toolbar().add_button(button, workflow_id).await?)
}

#[allow(clippy::too_many_arguments)]
pub async fn toolbar_update(
    daw: &Daw,
    command_name: &str,
    label: &str,
    target: &str,
    workflow_id: &str,
    position: Option<u32>,
    icon: Option<&str>,
    icon_kind: crate::service::ToolbarIconKind,
    flags: u32,
) -> Result<Value> {
    let button = toolbar_button(
        command_name,
        label,
        target,
        position,
        icon,
        icon_kind,
        flags,
    )?;
    toolbar_operation_json(daw.toolbar().update_button(button, workflow_id).await?)
}

pub async fn toolbar_remove(daw: &Daw, command_name: &str, target: &str) -> Result<Value> {
    toolbar_operation_json(
        daw.toolbar()
            .remove_button(parse_toolbar_target(target)?, command_name)
            .await?,
    )
}

pub async fn toolbar_move(
    daw: &Daw,
    command_name: &str,
    target: &str,
    position: u32,
) -> Result<Value> {
    toolbar_operation_json(
        daw.toolbar()
            .move_button(parse_toolbar_target(target)?, command_name, position)
            .await?,
    )
}

pub async fn toolbar_icon(
    daw: &Daw,
    command_name: &str,
    target: &str,
    icon: Option<&str>,
    icon_kind: crate::service::ToolbarIconKind,
    clear: bool,
) -> Result<Value> {
    if clear && icon.is_some() {
        eyre::bail!("use either --clear or --icon, not both");
    }
    if !clear && icon.is_none() {
        eyre::bail!("toolbar-icon requires --icon or --clear");
    }
    toolbar_operation_json(
        daw.toolbar()
            .set_button_icon(
                parse_toolbar_target(target)?,
                command_name,
                (!clear)
                    .then(|| make_toolbar_icon(icon, icon_kind))
                    .flatten(),
            )
            .await?,
    )
}

fn screenset_options(persist: bool) -> crate::service::ScreensetOptions {
    crate::service::ScreensetOptions {
        scope: crate::service::ScreensetScope::Global,
        persist,
    }
}

fn screenset_result_json(result: crate::service::ScreensetResult) -> Result<Value> {
    if result.ok {
        Ok(json!({
            "ok": true,
            "id": result.id,
        }))
    } else {
        match result.error {
            Some(message) => Err(eyre::eyre!(message)),
            None => Err(eyre::eyre!("screenset operation failed")),
        }
    }
}

pub async fn screenset_list(daw: &Daw) -> Result<Value> {
    let rows = daw.screensets().list(screenset_options(true)).await?;
    let json = facet_json::to_string(&rows)
        .map_err(|err| eyre::eyre!("serialize screenset list: {err}"))?;
    Ok(serde_json::from_str(&json)?)
}

#[allow(clippy::too_many_arguments)]
pub async fn screenset_capture(
    daw: &Daw,
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    kind: crate::service::ScreensetKind,
    tags: Vec<String>,
    actions_on_apply: Vec<String>,
    persist: bool,
) -> Result<Value> {
    let result = daw
        .screensets()
        .capture(
            id,
            name.unwrap_or(id),
            description.unwrap_or_default(),
            kind,
            tags,
            actions_on_apply,
            screenset_options(persist),
        )
        .await?;
    screenset_result_json(result)
}

pub async fn screenset_show(daw: &Daw, id: &str) -> Result<Value> {
    let Some(screenset) = daw.screensets().get(id, screenset_options(true)).await? else {
        eyre::bail!("screenset not found: {id}");
    };
    let json = facet_json::to_string(&screenset)
        .map_err(|err| eyre::eyre!("serialize screenset: {err}"))?;
    Ok(serde_json::from_str(&json)?)
}

pub async fn screenset_apply(daw: &Daw, id: &str) -> Result<Value> {
    screenset_result_json(daw.screensets().apply(id, screenset_options(true)).await?)
}

pub async fn screenset_delete(daw: &Daw, id: &str, persist: bool) -> Result<Value> {
    screenset_result_json(
        daw.screensets()
            .delete(id, screenset_options(persist))
            .await?,
    )
}

pub async fn rpp_summary(daw: &Daw, path: &str) -> Result<Value> {
    let summary = daw.dawfile().summarize_project(path).await?;
    if !summary.error.is_empty() {
        eyre::bail!(summary.error);
    }
    Ok(json!({
        "path": summary.path,
        "version": summary.version,
        "version_string": summary.version_string,
        "track_count": summary.track_count,
        "marker_count": summary.marker_count,
        "region_count": summary.region_count,
        "tracks": summary.tracks.iter().map(|t| json!({
            "name": t.name,
            "items": t.item_count,
            "fx_count": t.fx_count,
        })).collect::<Vec<_>>(),
    }))
}

pub async fn combine_rpl(
    daw: &Daw,
    input: &str,
    output: Option<&str>,
    gap_measures: u32,
) -> Result<Value> {
    let result = daw
        .dawfile()
        .combine_setlist(
            input,
            output.unwrap_or(""),
            crate::service::CombineSetlistOptions { gap_measures },
        )
        .await?;
    if !result.error.is_empty() {
        eyre::bail!(result.error);
    }
    Ok(json!({
        "input": result.input,
        "output": result.output,
        "song_count": result.song_count,
        "gap_measures": result.gap_measures,
        "songs": result.songs.iter().map(|s| json!({
            "index": s.index,
            "name": s.name,
            "global_start_seconds": s.global_start_seconds,
            "duration_seconds": s.duration_seconds,
        })).collect::<Vec<_>>(),
        "total_seconds": result.total_seconds,
    }))
}

// =============================================================================
// Reified-op execution — the agent-facing generic surface
// =============================================================================

/// Names accepted by `daw call <service>.<method>` mapped to the
/// `BatchOp` wrapper key, one entry per ops-covered service.
const OP_SERVICES: &[(&str, &str)] = &[
    ("transport", "Transport"),
    ("project", "Project"),
    ("projects", "Project"),
    ("track", "Track"),
    ("tracks", "Track"),
    ("marker", "Marker"),
    ("markers", "Marker"),
    ("fx", "Fx"),
    ("effects", "Fx"),
    ("routing", "Routing"),
    ("region", "Region"),
    ("regions", "Region"),
    ("tempo_map", "TempoMap"),
    ("ext_state", "ExtState"),
    ("item", "Item"),
    ("items", "Item"),
    ("take", "Take"),
    ("takes", "Take"),
];

fn parse_response_json(response: &crate::service::batch::BatchResponse) -> Result<Value> {
    let json = facet_json::to_string(response)
        .map_err(|e| eyre::eyre!("serialize batch response: {e}"))?;
    Ok(serde_json::from_str(&json)?)
}

/// Execute a whole JSON batch program in one RPC round trip. The
/// program is a facet-JSON `BatchRequest`; see `daw service-catalog`
/// for methods and `daw op` for the per-op shape.
pub async fn run_batch(daw: &Daw, program_json: &str) -> Result<Value> {
    let request: crate::service::batch::BatchRequest = facet_json::from_str(program_json)
        .map_err(|e| eyre::eyre!("invalid batch program JSON: {e}"))?;
    let response = daw.execute_batch(request).await?;
    parse_response_json(&response)
}

/// Execute one reified op (externally-tagged JSON, e.g.
/// `{"Marker":{"Add":{"project":{"Literal":"Current"},"position":1.5,"name":"x"}}}`).
pub async fn run_op(daw: &Daw, op_json: &str) -> Result<Value> {
    let op: crate::service::batch::BatchOp = facet_json::from_str(op_json).map_err(|e| {
        eyre::eyre!(
            "invalid op JSON: {e}\n(expected {{\"<Service>\":{{\"<Method>\":{{..args..}}}}}}; \
             see `daw service-catalog` for methods)"
        )
    })?;
    let request = crate::service::batch::BatchRequest {
        instructions: vec![crate::service::batch::BatchInstruction { step: 0, op }],
        options: crate::service::batch::BatchOptions::default(),
    };
    let response = daw.execute_batch(request).await?;
    let value = parse_response_json(&response)?;
    // Single-op sugar: unwrap to the one outcome.
    Ok(value
        .get("results")
        .and_then(|r| r.get(0))
        .and_then(|r| r.get("outcome"))
        .cloned()
        .unwrap_or(value))
}

/// `daw call <service>.<method> --args '<json>'` — assembles the
/// externally-tagged op from a human-friendly target name and runs it.
pub async fn run_call(daw: &Daw, target: &str, args_json: Option<&str>) -> Result<Value> {
    let (service, method) = target.split_once('.').ok_or_else(|| {
        eyre::eyre!("call target must be <service>.<method>, e.g. transport.play")
    })?;
    let key = OP_SERVICES
        .iter()
        .find(|(name, _)| *name == service.to_lowercase())
        .map(|(_, key)| *key)
        .ok_or_else(|| {
            let mut names: Vec<&str> = OP_SERVICES.iter().map(|(n, _)| *n).collect();
            names.dedup();
            eyre::eyre!("unknown service `{service}` — one of: {}", names.join(", "))
        })?;
    // snake_case method → PascalCase op variant.
    let variant: String = method
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect();
    let args = args_json.unwrap_or("{}");
    let op_json = format!("{{\"{key}\":{{\"{variant}\":{args}}}}}");
    match run_op(daw, &op_json).await {
        Ok(v) => Ok(v),
        // Ergonomic default: most methods take a `project` argument; if
        // the call failed and the caller didn't pass one, retry with the
        // current project injected. On a second failure report the
        // original error (the injected field may itself be the excess).
        Err(original) => match args_json.map(str::trim) {
            None | Some("{}") | Some("") => {
                let with_project = format!(
                    "{{\"{key}\":{{\"{variant}\":{{\"project\":{{\"Literal\":\"Current\"}}}}}}}}"
                );
                run_op(daw, &with_project).await.map_err(|_| original)
            }
            Some(args) if !args.contains("\"project\"") => {
                let inner = args.trim_start_matches('{');
                let with_project = format!(
                    "{{\"{key}\":{{\"{variant}\":{{\"project\":{{\"Literal\":\"Current\"}},{inner}}}}}}}"
                );
                run_op(daw, &with_project).await.map_err(|_| original)
            }
            _ => Err(original),
        },
    }
}
