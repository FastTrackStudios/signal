//! Container-aware FX tree construction + node-id ↔ raw-index
//! resolution. REAPER's container addressing is stride-based; we use
//! its v7.06+ `container_item.X` named config to walk safely.

use daw_proto::{FxContainerChannelConfig, FxNode, FxNodeId, FxRoutingMode};
use reaper_high::FxChain;
use reaper_medium::TrackFxLocation;
use tracing::warn;

use super::util::{build_fx_info, fx_location, read_config_str, read_config_u32, safe_fx_call};

/// REAPER container addressing base offset for child slots.
pub const CONTAINER_BASE: u32 = 0x2000000;

/// Build the top-level node list for an FX chain. Recurses into
/// containers via [`build_container_node`].
pub fn build_fx_tree_from_chain(
    chain: &FxChain,
    _is_input: bool,
    top_level_count: u32,
) -> Vec<FxNode> {
    let mut nodes = Vec::new();

    for i in 0..top_level_count {
        let fx = chain.fx_by_index_untracked(i);
        let path_prefix = format!("{}", i);

        if is_container_fx(&fx) {
            nodes.push(build_container_node(chain, &fx, i, None, &path_prefix));
        } else {
            nodes.push(build_plugin_node(chain, &fx, None));
        }
    }

    nodes
}

/// Detect whether an FX slot is a container using three independent
/// signals: `fx_type` config, `container_count` config, and
/// `info().sub_type_expression`. Containers don't always cleanly
/// stringify through raw `get_named_config_param`, so we belt-and-brace.
pub fn is_container_fx(fx: &reaper_high::Fx) -> bool {
    if let Some(ft) = read_config_str(fx, "fx_type")
        && ft == "Container"
    {
        return true;
    }

    if let Some(cc) = read_config_str(fx, "container_count") {
        if cc.parse::<u32>().unwrap_or(0) > 0 {
            return true;
        }
        if cc.parse::<u32>().is_ok() {
            return true;
        }
    }

    if let Ok(info) = fx.info()
        && info.sub_type_expression == "Container"
    {
        return true;
    }

    false
}

/// Get the raw encoded FX index for child `child_index` of `container_fx`
/// via REAPER's v7.06+ `container_item.X` named config.
pub fn container_child_fx_id(container_fx: &reaper_high::Fx, child_index: u32) -> Option<u32> {
    let key = format!("container_item.{}", child_index);
    read_config_str(container_fx, &key).and_then(|s| s.parse::<u32>().ok())
}

/// Verify the actual child count of a container matches `expected_count`.
/// Used after structural mutations to detect silent stride-addressing
/// failures.
pub fn verify_container_child_count(
    chain: &FxChain,
    container_index: u32,
    expected_count: u32,
) -> Result<(), String> {
    let container_fx = chain.fx_by_index_untracked(container_index);
    let actual = read_config_u32(&container_fx, "container_count");
    if actual != expected_count {
        Err(format!(
            "container at index {} has {} children, expected {}",
            container_index, actual, expected_count
        ))
    } else {
        Ok(())
    }
}

fn build_plugin_node(chain: &FxChain, fx: &reaper_high::Fx, parent_id: Option<FxNodeId>) -> FxNode {
    let fx_info = build_fx_info(fx, Some(chain));
    let enabled = fx_info.enabled;
    let guid = fx_info.guid.clone();
    FxNode::plugin(FxNodeId::from_guid(guid), fx_info, enabled, parent_id)
}

fn build_container_node(
    chain: &FxChain,
    container_fx: &reaper_high::Fx,
    _container_flat_index: u32,
    parent_id: Option<FxNodeId>,
    path: &str,
) -> FxNode {
    let container_id = FxNodeId::container(path);

    let child_count = read_config_u32(container_fx, "container_count");
    let routing = read_config_str(container_fx, "parallel")
        .map(|s| FxRoutingMode::from_reaper_param(&s))
        .unwrap_or_default();
    let channel_config = FxContainerChannelConfig {
        nch: read_config_u32(container_fx, "container_nch"),
        nch_in: read_config_u32(container_fx, "container_nch_in"),
        nch_out: read_config_u32(container_fx, "container_nch_out"),
    };

    let name = read_config_str(container_fx, "renamed_name")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            safe_fx_call("container.name", None, || {
                container_fx.name().to_str().to_string()
            })
            .unwrap_or_else(|| "Container".to_string())
        });

    let enabled =
        safe_fx_call("container.is_enabled", None, || container_fx.is_enabled()).unwrap_or(true);

    let mut children = Vec::new();
    for i in 0..child_count {
        let Some(child_raw) = container_child_fx_id(container_fx, i) else {
            warn!(
                "container_item.{} not found in container at path {}",
                i, path
            );
            continue;
        };

        let child_fx = chain.fx_by_index_untracked(child_raw);
        let child_path = format!("{}:{}", path, i);

        if is_container_fx(&child_fx) {
            children.push(build_container_node(
                chain,
                &child_fx,
                child_raw,
                Some(container_id.clone()),
                &child_path,
            ));
        } else {
            children.push(build_plugin_node(
                chain,
                &child_fx,
                Some(container_id.clone()),
            ));
        }
    }

    let mut node = FxNode::container(
        container_id,
        name,
        routing,
        channel_config,
        enabled,
        parent_id,
    );
    if let Some(c) = node.children_mut() {
        *c = children;
    }
    node
}

/// Resolve an `FxNodeId` to a raw REAPER FX index (chain-local). Plugin
/// nodes resolve by GUID scan, container nodes by path traversal.
pub fn resolve_node_to_raw_index(chain: &FxChain, node_id: &FxNodeId) -> Option<u32> {
    if node_id.is_container() {
        resolve_container_path(chain, node_id)
    } else {
        resolve_plugin_guid(chain, node_id)
    }
}

fn resolve_container_path(chain: &FxChain, node_id: &FxNodeId) -> Option<u32> {
    let path_str = node_id.as_str().strip_prefix("container:")?;
    let segments: Vec<u32> = path_str.split(':').filter_map(|s| s.parse().ok()).collect();

    if segments.is_empty() {
        return None;
    }

    let top_index = segments[0];
    if top_index >= chain.fx_count() {
        return None;
    }

    if segments.len() == 1 {
        return Some(top_index);
    }

    let mut current_addr = top_index;
    for &child_pos in &segments[1..] {
        let container_fx = chain.fx_by_index_untracked(current_addr);
        let child_raw = container_child_fx_id(&container_fx, child_pos)?;
        current_addr = child_raw;
    }

    Some(current_addr)
}

/// Resolve a plugin `FxNodeId` (GUID) to its raw chain index, scanning
/// containers if necessary.
pub fn resolve_plugin_guid(chain: &FxChain, node_id: &FxNodeId) -> Option<u32> {
    let target_guid = node_id.as_str();

    let top_count = chain.fx_count();
    for i in 0..top_count {
        let guid = reaper_high::get_fx_guid(chain, i).map(|g| g.to_string_without_braces());
        if guid.as_deref() == Some(target_guid) {
            return Some(i);
        }
    }

    scan_containers_for_guid(chain, target_guid, top_count)
}

fn scan_containers_for_guid(chain: &FxChain, target_guid: &str, top_count: u32) -> Option<u32> {
    for i in 0..top_count {
        let fx = chain.fx_by_index_untracked(i);
        if is_container_fx(&fx)
            && let Some(raw) = scan_children_for_guid(chain, &fx, target_guid)
        {
            return Some(raw);
        }
    }
    None
}

fn scan_children_for_guid(
    chain: &FxChain,
    container_fx: &reaper_high::Fx,
    target_guid: &str,
) -> Option<u32> {
    let child_count = read_config_u32(container_fx, "container_count");

    for i in 0..child_count {
        let Some(child_raw) = container_child_fx_id(container_fx, i) else {
            continue;
        };
        let child_fx = chain.fx_by_index_untracked(child_raw);

        if is_container_fx(&child_fx) {
            if let Some(raw) = scan_children_for_guid(chain, &child_fx, target_guid) {
                return Some(raw);
            }
        } else {
            let guid =
                reaper_high::get_fx_guid(chain, child_raw).map(|g| g.to_string_without_braces());
            if guid.as_deref() == Some(target_guid) {
                return Some(child_raw);
            }
        }
    }
    None
}

#[allow(dead_code)]
pub fn resolve_node_to_location(
    chain: &FxChain,
    node_id: &FxNodeId,
    is_input: bool,
) -> Option<TrackFxLocation> {
    let raw = resolve_node_to_raw_index(chain, node_id)?;
    Some(fx_location(raw, is_input))
}
