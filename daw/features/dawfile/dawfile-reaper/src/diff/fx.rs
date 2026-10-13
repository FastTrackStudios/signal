//! FX chain diffing — GUID-based matching with recursive container support.

use std::collections::HashMap;

use super::envelope;
use super::types::*;
use crate::types::{FxChain, FxChainNode, FxContainer, FxPlugin};

pub(crate) fn diff_fx_chains(old: Option<&FxChain>, new: Option<&FxChain>) -> Option<FxChainDiff> {
    match (old, new) {
        (None, None) => None,
        (None, Some(_)) => Some(FxChainDiff {
            property_changes: vec![PropertyChange {
                field: "fx_chain".into(),
                old_value: "none".into(),
                new_value: "present".into(),
            }],
            nodes: Vec::new(),
        }),
        (Some(_), None) => Some(FxChainDiff {
            property_changes: vec![PropertyChange {
                field: "fx_chain".into(),
                old_value: "present".into(),
                new_value: "none".into(),
            }],
            nodes: Vec::new(),
        }),
        (Some(old_fx), Some(new_fx)) => {
            let mut prop_changes = Vec::new();
            if old_fx.show != new_fx.show {
                prop_changes.push(PropertyChange {
                    field: "show".into(),
                    old_value: old_fx.show.to_string(),
                    new_value: new_fx.show.to_string(),
                });
            }
            if old_fx.docked != new_fx.docked {
                prop_changes.push(PropertyChange {
                    field: "docked".into(),
                    old_value: old_fx.docked.to_string(),
                    new_value: new_fx.docked.to_string(),
                });
            }

            let node_diffs = diff_fx_nodes(&old_fx.nodes, &new_fx.nodes);

            if prop_changes.is_empty() && node_diffs.is_empty() {
                None
            } else {
                Some(FxChainDiff {
                    property_changes: prop_changes,
                    nodes: node_diffs,
                })
            }
        }
    }
}

fn diff_fx_nodes(old: &[FxChainNode], new: &[FxChainNode]) -> Vec<FxNodeDiff> {
    let mut diffs = Vec::new();

    // Extract fxid for matching
    let old_map: HashMap<Option<&str>, (usize, &FxChainNode)> = old
        .iter()
        .enumerate()
        .map(|(i, n)| (node_fxid(n), (i, n)))
        .collect();

    let new_map: HashMap<Option<&str>, (usize, &FxChainNode)> = new
        .iter()
        .enumerate()
        .map(|(i, n)| (node_fxid(n), (i, n)))
        .collect();

    // Removed or modified
    for (&key, &(_, old_node)) in &old_map {
        match new_map.get(&key) {
            None => {
                diffs.push(FxNodeDiff {
                    fxid: key.map(String::from),
                    name: node_name(old_node),
                    kind: ChangeKind::Removed,
                    property_changes: Vec::new(),
                    state_changed: false,
                    param_envelope_changes: Vec::new(),
                    children: None,
                });
            }
            Some(&(_, new_node)) => {
                if let Some(diff) = diff_single_node(old_node, new_node) {
                    diffs.push(diff);
                }
            }
        }
    }

    // Added
    for (&key, &(_, new_node)) in &new_map {
        if !old_map.contains_key(&key) {
            diffs.push(FxNodeDiff {
                fxid: key.map(String::from),
                name: node_name(new_node),
                kind: ChangeKind::Added,
                property_changes: Vec::new(),
                state_changed: false,
                param_envelope_changes: Vec::new(),
                children: None,
            });
        }
    }

    diffs
}

fn diff_single_node(old: &FxChainNode, new: &FxChainNode) -> Option<FxNodeDiff> {
    match (old, new) {
        (FxChainNode::Plugin(old_p), FxChainNode::Plugin(new_p)) => diff_plugins(old_p, new_p),
        (FxChainNode::Container(old_c), FxChainNode::Container(new_c)) => {
            diff_containers(old_c, new_c)
        }
        // Type changed (plugin ↔ container) — treat as remove + add
        _ => Some(FxNodeDiff {
            fxid: node_fxid(new).map(String::from),
            name: node_name(new),
            kind: ChangeKind::Modified,
            property_changes: vec![PropertyChange {
                field: "node_type".into(),
                old_value: node_type_name(old).into(),
                new_value: node_type_name(new).into(),
            }],
            state_changed: true,
            param_envelope_changes: Vec::new(),
            children: None,
        }),
    }
}

fn diff_plugins(old: &FxPlugin, new: &FxPlugin) -> Option<FxNodeDiff> {
    let mut props = Vec::new();

    if old.name != new.name {
        props.push(PropertyChange {
            field: "name".into(),
            old_value: old.name.clone(),
            new_value: new.name.clone(),
        });
    }
    if old.bypassed != new.bypassed {
        props.push(PropertyChange {
            field: "bypassed".into(),
            old_value: old.bypassed.to_string(),
            new_value: new.bypassed.to_string(),
        });
    }
    if old.offline != new.offline {
        props.push(PropertyChange {
            field: "offline".into(),
            old_value: old.offline.to_string(),
            new_value: new.offline.to_string(),
        });
    }
    if old.preset_name != new.preset_name {
        props.push(PropertyChange {
            field: "preset_name".into(),
            old_value: old.preset_name.clone().unwrap_or_default(),
            new_value: new.preset_name.clone().unwrap_or_default(),
        });
    }

    let state_changed = old.state_data != new.state_data;

    let env_changes = diff_param_envelopes(old, new);

    if props.is_empty() && !state_changed && env_changes.is_empty() {
        None
    } else {
        Some(FxNodeDiff {
            fxid: new.fxid.clone(),
            name: new.name.clone(),
            kind: ChangeKind::Modified,
            property_changes: props,
            state_changed,
            param_envelope_changes: env_changes,
            children: None,
        })
    }
}

fn diff_param_envelopes(old: &FxPlugin, new: &FxPlugin) -> Vec<EnvelopeDiff> {
    // FxParamEnvelope has its own point type (FxEnvelopePoint), not Envelope.
    // Match by param index and detect added/removed/modified envelopes.
    let mut diffs = Vec::new();

    let old_map: HashMap<u32, &crate::types::FxParamEnvelope> = old
        .param_envelopes
        .iter()
        .map(|pe| (pe.param.index, pe))
        .collect();

    let new_map: HashMap<u32, &crate::types::FxParamEnvelope> = new
        .param_envelopes
        .iter()
        .map(|pe| (pe.param.index, pe))
        .collect();

    for (&idx, &old_pe) in &old_map {
        match new_map.get(&idx) {
            None => {
                diffs.push(EnvelopeDiff {
                    guid: old_pe.eguid.clone().unwrap_or_default(),
                    envelope_type: format!("param_{}", idx),
                    kind: ChangeKind::Removed,
                    property_changes: Vec::new(),
                    point_changes: Vec::new(),
                    automation_item_changes: Vec::new(),
                });
            }
            Some(&new_pe) => {
                let mut changes = Vec::new();
                if old_pe.active != new_pe.active {
                    changes.push(PropertyChange {
                        field: "active".into(),
                        old_value: old_pe.active.to_string(),
                        new_value: new_pe.active.to_string(),
                    });
                }
                if old_pe.points != new_pe.points {
                    changes.push(PropertyChange {
                        field: "points".into(),
                        old_value: format!("{} points", old_pe.points.len()),
                        new_value: format!("{} points", new_pe.points.len()),
                    });
                }
                if !changes.is_empty() {
                    diffs.push(EnvelopeDiff {
                        guid: new_pe.eguid.clone().unwrap_or_default(),
                        envelope_type: format!("param_{}", idx),
                        kind: ChangeKind::Modified,
                        property_changes: changes,
                        point_changes: Vec::new(),
                        automation_item_changes: Vec::new(),
                    });
                }
            }
        }
    }

    for (&idx, &new_pe) in &new_map {
        if !old_map.contains_key(&idx) {
            diffs.push(EnvelopeDiff {
                guid: new_pe.eguid.clone().unwrap_or_default(),
                envelope_type: format!("param_{}", idx),
                kind: ChangeKind::Added,
                property_changes: Vec::new(),
                point_changes: Vec::new(),
                automation_item_changes: Vec::new(),
            });
        }
    }

    diffs
}

fn diff_containers(old: &FxContainer, new: &FxContainer) -> Option<FxNodeDiff> {
    let mut props = Vec::new();
    if old.name != new.name {
        props.push(PropertyChange {
            field: "name".into(),
            old_value: old.name.clone(),
            new_value: new.name.clone(),
        });
    }
    if old.bypassed != new.bypassed {
        props.push(PropertyChange {
            field: "bypassed".into(),
            old_value: old.bypassed.to_string(),
            new_value: new.bypassed.to_string(),
        });
    }

    let child_diffs = diff_fx_nodes(&old.children, &new.children);
    let children = if child_diffs.is_empty() {
        None
    } else {
        Some(FxChainDiff {
            property_changes: Vec::new(),
            nodes: child_diffs,
        })
    };

    if props.is_empty() && children.is_none() {
        None
    } else {
        Some(FxNodeDiff {
            fxid: None,
            name: new.name.clone(),
            kind: ChangeKind::Modified,
            property_changes: props,
            state_changed: false,
            param_envelope_changes: Vec::new(),
            children,
        })
    }
}

fn node_fxid(node: &FxChainNode) -> Option<&str> {
    match node {
        FxChainNode::Plugin(p) => p.fxid.as_deref(),
        FxChainNode::Container(_) => None,
    }
}

fn node_name(node: &FxChainNode) -> String {
    match node {
        FxChainNode::Plugin(p) => p.name.clone(),
        FxChainNode::Container(c) => c.name.clone(),
    }
}

fn node_type_name(node: &FxChainNode) -> &str {
    match node {
        FxChainNode::Plugin(_) => "plugin",
        FxChainNode::Container(_) => "container",
    }
}
