//! Workflow-based config composition engine with smart conflict resolution.
//!
//! Keymaps are composed from independent workflow units. Each workflow defines
//! its own keybindings, and users enable/disable workflows to build their keymap.
//! Conflicts are detected, classified by severity, and auto-resolved with
//! ranked suggestions.
//!
//! # Resolution Strategies (ranked by ergonomic preference)
//!
//! 1. **Context isolation**: No real conflict if workflows target different contexts
//! 2. **Priority wins**: Higher-priority workflow keeps the key, lower gets suggestion
//! 3. **Which-key prefix**: Move conflicting workflow under a prefix key (e.g., "v" prefix)
//! 4. **Modifier addition**: Add Ctrl/Shift/Alt to the lower-priority workflow
//! 5. **Key swap**: Swap the conflicting key between workflows
//! 6. **Individual remap**: Remap just the conflicting binding
//! 7. **Disable**: Drop the lower-priority binding entirely

use std::collections::{HashMap, HashSet};

use crate::keybind_config::types::KeybindDef;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A composable workflow unit.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowBindings {
    pub id: String,
    pub name: String,
    pub bindings: Vec<KeybindDef>,
    pub default_enabled: bool,
    /// Higher priority wins on unresolved conflicts.
    pub priority: i32,
}

/// Override rules for a workflow's bindings.
#[derive(Clone, Debug, Default, PartialEq, facet::Facet)]
pub struct WorkflowOverride {
    pub workflow: String,
    /// Move all bindings under a which-key prefix.
    /// e.g., prefix "v" → "h" becomes "vh", "j" becomes "vj".
    pub prefix: Option<String>,
    /// Remap specific keys: vec of (from, to) pairs.
    pub remaps: Option<Vec<KeyRemap>>,
    /// Add modifier to all bindings (e.g., "C-", "S-", "A-").
    pub add_modifier: Option<String>,
    /// Swap a key with another workflow.
    pub swap_with: Option<KeySwap>,
    /// Disable specific bindings by key.
    pub disabled_keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, facet::Facet)]
pub struct KeyRemap {
    pub from: String,
    pub to: String,
}

/// Swap keys between two workflows.
#[derive(Clone, Debug, PartialEq, facet::Facet)]
pub struct KeySwap {
    pub other_workflow: String,
    /// The key in this workflow to give to the other.
    pub give_key: String,
    /// The key to receive from the other workflow.
    pub receive_key: String,
}

// ---------------------------------------------------------------------------
// Conflict types
// ---------------------------------------------------------------------------

/// How severe a conflict is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConflictSeverity {
    /// Different contexts — not a real conflict.
    None,
    /// One is a prefix of the other (e.g., "g" vs "gg") — partial overlap.
    PrefixOverlap,
    /// Same key, same context — hard conflict.
    Hard,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub keys: String,
    pub context: Option<String>,
    pub severity: ConflictSeverity,
    pub workflow_a: String,
    pub action_a: String,
    pub desc_a: Option<String>,
    pub workflow_b: String,
    pub action_b: String,
    pub desc_b: Option<String>,
}

/// A group of related conflicts between two workflows.
#[derive(Clone, Debug, PartialEq)]
pub struct ConflictGroup {
    pub workflow_a: String,
    pub workflow_b: String,
    /// All conflicting keys between these two workflows.
    pub conflicts: Vec<Conflict>,
    /// Auto-generated resolution suggestions, ranked by quality.
    pub suggestions: Vec<ResolutionSuggestion>,
}

// ---------------------------------------------------------------------------
// Resolution suggestions
// ---------------------------------------------------------------------------

/// A suggested resolution for a conflict group.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolutionSuggestion {
    /// Human-readable description of what this does.
    pub description: String,
    /// Quality score (higher = better). Based on ergonomics.
    pub score: i32,
    /// The override to apply.
    pub override_rule: WorkflowOverride,
    /// Which strategy this uses.
    pub strategy: ResolutionStrategy,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResolutionStrategy {
    /// Higher priority workflow wins, lower priority's conflicting keys disabled.
    PriorityWins,
    /// Move entire workflow under a which-key prefix.
    WhichKeyPrefix { prefix: String },
    /// Add a modifier to all bindings in the workflow.
    AddModifier { modifier: String },
    /// Remap a specific key.
    RemapKey { from: String, to: String },
    /// Swap keys between workflows.
    SwapKeys { key_a: String, key_b: String },
    /// Disable specific bindings.
    DisableBindings { keys: Vec<String> },
}

// ---------------------------------------------------------------------------
// Composed output
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ComposedConfig {
    pub bindings: Vec<ComposedBinding>,
    pub conflicts: Vec<ConflictGroup>,
    pub active_workflows: Vec<String>,
    pub stats: CompositionStats,
}

#[derive(Clone, Debug, Default)]
pub struct CompositionStats {
    pub total_bindings: usize,
    pub remapped_bindings: usize,
    pub disabled_bindings: usize,
    pub hard_conflicts: usize,
    pub resolved_conflicts: usize,
}

#[derive(Clone, Debug)]
pub struct ComposedBinding {
    pub keys: String,
    pub action: String,
    pub desc: Option<String>,
    pub source_workflow: String,
    pub remapped: bool,
    pub context: Option<String>,
}

// ---------------------------------------------------------------------------
// Composition engine
// ---------------------------------------------------------------------------

pub fn compose(
    workflows: &[WorkflowBindings],
    overrides: &[WorkflowOverride],
    enabled_ids: &[String],
) -> ComposedConfig {
    let override_map: HashMap<&str, &WorkflowOverride> =
        overrides.iter().map(|o| (o.workflow.as_str(), o)).collect();

    let mut composed: Vec<ComposedBinding> = Vec::new();
    let mut key_owners: HashMap<(String, Option<String>), (String, String, Option<String>)> =
        HashMap::new();
    let mut raw_conflicts: Vec<Conflict> = Vec::new();
    let mut stats = CompositionStats::default();

    let mut enabled: Vec<&WorkflowBindings> = workflows
        .iter()
        .filter(|w| enabled_ids.contains(&w.id))
        .collect();
    enabled.sort_by_key(|w| w.priority);

    for workflow in &enabled {
        let overr = override_map.get(workflow.id.as_str());

        for binding in &workflow.bindings {
            // Check disabled
            if let Some(ov) = overr
                && ov.disabled_keys.contains(&binding.keys)
            {
                stats.disabled_bindings += 1;
                continue;
            }

            let final_keys = apply_override(&binding.keys, overr);
            let remapped = final_keys != binding.keys;
            if remapped {
                stats.remapped_bindings += 1;
            }

            let ctx = binding.context.as_ref().map(|c| format!("{c:?}"));
            let map_key = (final_keys.clone(), ctx.clone());

            if let Some((existing_wf, existing_action, existing_desc)) = key_owners.get(&map_key)
                && existing_wf != &workflow.id
            {
                raw_conflicts.push(Conflict {
                    keys: final_keys.clone(),
                    context: ctx.clone(),
                    severity: ConflictSeverity::Hard,
                    workflow_a: existing_wf.clone(),
                    action_a: existing_action.clone(),
                    desc_a: existing_desc.clone(),
                    workflow_b: workflow.id.clone(),
                    action_b: binding.action.clone(),
                    desc_b: binding.desc.clone(),
                });
            }

            key_owners.insert(
                map_key,
                (
                    workflow.id.clone(),
                    binding.action.clone(),
                    binding.desc.clone(),
                ),
            );

            composed.push(ComposedBinding {
                keys: final_keys,
                action: binding.action.clone(),
                desc: binding.desc.clone(),
                source_workflow: workflow.id.clone(),
                remapped,
                context: ctx,
            });
            stats.total_bindings += 1;
        }
    }

    // Detect prefix overlaps
    detect_prefix_overlaps(&composed, &mut raw_conflicts);

    // Group conflicts by workflow pair and generate suggestions
    let conflict_groups = group_and_suggest(workflows, raw_conflicts);
    stats.hard_conflicts = conflict_groups.iter().map(|g| g.conflicts.len()).sum();

    ComposedConfig {
        bindings: composed,
        conflicts: conflict_groups,
        active_workflows: enabled.iter().map(|w| w.id.clone()).collect(),
        stats,
    }
}

fn apply_override(keys: &str, overr: Option<&&WorkflowOverride>) -> String {
    let Some(ov) = overr else {
        return keys.to_string();
    };
    let mut result = keys.to_string();

    // Apply prefix
    if let Some(ref prefix) = ov.prefix
        && !result.starts_with('<')
    {
        result = format!("{prefix}{result}");
    }

    // Apply specific remaps
    if let Some(ref remaps) = ov.remaps {
        for remap in remaps {
            if result.starts_with(&remap.from) {
                result = format!("{}{}", remap.to, &result[remap.from.len()..]);
                break;
            }
        }
    }

    // Apply modifier addition
    if let Some(ref modifier) = ov.add_modifier {
        if !result.starts_with('<') {
            result = format!("<{modifier}{result}>");
        } else if result.starts_with('<') && result.ends_with('>') {
            let inner = &result[1..result.len() - 1];
            result = format!("<{modifier}{inner}>");
        }
    }

    result
}

/// Detect when one binding's key is a prefix of another.
fn detect_prefix_overlaps(bindings: &[ComposedBinding], conflicts: &mut Vec<Conflict>) {
    let keys: Vec<(&str, &str, Option<&str>)> = bindings
        .iter()
        .map(|b| {
            (
                b.keys.as_str(),
                b.source_workflow.as_str(),
                b.context.as_deref(),
            )
        })
        .collect();

    for (i, (key_a, wf_a, ctx_a)) in keys.iter().enumerate() {
        for (key_b, wf_b, ctx_b) in keys.iter().skip(i + 1) {
            if wf_a == wf_b {
                continue;
            }
            if ctx_a != ctx_b {
                continue;
            }
            // Check if one is a prefix of the other (and they're not equal)
            if key_a != key_b && (key_a.starts_with(key_b) || key_b.starts_with(key_a)) {
                conflicts.push(Conflict {
                    keys: format!("{key_a} / {key_b}"),
                    context: ctx_a.map(String::from),
                    severity: ConflictSeverity::PrefixOverlap,
                    workflow_a: wf_a.to_string(),
                    action_a: String::new(),
                    desc_a: Some(format!("prefix overlap: {key_a}")),
                    workflow_b: wf_b.to_string(),
                    action_b: String::new(),
                    desc_b: Some(format!("prefix overlap: {key_b}")),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Smart suggestion engine
// ---------------------------------------------------------------------------

fn group_and_suggest(
    workflows: &[WorkflowBindings],
    conflicts: Vec<Conflict>,
) -> Vec<ConflictGroup> {
    // Group by workflow pair
    let mut groups: HashMap<(String, String), Vec<Conflict>> = HashMap::new();
    for c in conflicts {
        let key = if c.workflow_a < c.workflow_b {
            (c.workflow_a.clone(), c.workflow_b.clone())
        } else {
            (c.workflow_b.clone(), c.workflow_a.clone())
        };
        groups.entry(key).or_default().push(c);
    }

    let wf_map: HashMap<&str, &WorkflowBindings> =
        workflows.iter().map(|w| (w.id.as_str(), w)).collect();

    groups
        .into_iter()
        .map(|((wf_a, wf_b), conflicts)| {
            let suggestions = generate_suggestions(
                &wf_a,
                &wf_b,
                &conflicts,
                wf_map.get(wf_a.as_str()),
                wf_map.get(wf_b.as_str()),
            );
            ConflictGroup {
                workflow_a: wf_a,
                workflow_b: wf_b,
                conflicts,
                suggestions,
            }
        })
        .collect()
}

fn generate_suggestions(
    wf_a_id: &str,
    wf_b_id: &str,
    conflicts: &[Conflict],
    wf_a: Option<&&WorkflowBindings>,
    wf_b: Option<&&WorkflowBindings>,
) -> Vec<ResolutionSuggestion> {
    let mut suggestions = Vec::new();
    let conflicting_keys: Vec<&str> = conflicts.iter().map(|c| c.keys.as_str()).collect();

    // Determine which workflow has lower priority (gets modified)
    let (loser_id, winner_id) = match (wf_a, wf_b) {
        (Some(a), Some(b)) if a.priority >= b.priority => (wf_b_id, wf_a_id),
        _ => (wf_a_id, wf_b_id),
    };

    // Strategy 1: Priority wins — just disable conflicting keys on lower priority
    suggestions.push(ResolutionSuggestion {
        description: format!(
            "Let '{}' win — disable {} conflicting key(s) from '{}'",
            winner_id,
            conflicting_keys.len(),
            loser_id
        ),
        score: 60,
        override_rule: WorkflowOverride {
            workflow: loser_id.to_string(),
            disabled_keys: conflicting_keys.iter().map(|k| k.to_string()).collect(),
            ..Default::default()
        },
        strategy: ResolutionStrategy::PriorityWins,
    });

    // Strategy 2: Which-key prefix — move loser under a prefix
    let used_single_keys: HashSet<&str> = conflicting_keys.iter().copied().collect();
    for prefix_candidate in ["v", "z", "\\", "'", ",", ".", "/"] {
        if !used_single_keys.contains(prefix_candidate) {
            suggestions.push(ResolutionSuggestion {
                description: format!(
                    "Move '{}' under '{prefix_candidate}' prefix — e.g., 'h' → '{prefix_candidate}h'",
                    loser_id,
                ),
                score: 80,
                override_rule: WorkflowOverride {
                    workflow: loser_id.to_string(),
                    prefix: Some(prefix_candidate.to_string()),
                    ..Default::default()
                },
                strategy: ResolutionStrategy::WhichKeyPrefix {
                    prefix: prefix_candidate.to_string(),
                },
            });
            break; // Only suggest the best prefix
        }
    }

    // Strategy 3: Add Ctrl modifier to loser
    suggestions.push(ResolutionSuggestion {
        description: format!("Add Ctrl to '{}' — e.g., 'h' → '<C-h>'", loser_id),
        score: 70,
        override_rule: WorkflowOverride {
            workflow: loser_id.to_string(),
            add_modifier: Some("C-".to_string()),
            ..Default::default()
        },
        strategy: ResolutionStrategy::AddModifier {
            modifier: "C-".to_string(),
        },
    });

    // Strategy 4: Add Alt modifier
    suggestions.push(ResolutionSuggestion {
        description: format!("Add Alt to '{}' — e.g., 'h' → '<A-h>'", loser_id),
        score: 65,
        override_rule: WorkflowOverride {
            workflow: loser_id.to_string(),
            add_modifier: Some("A-".to_string()),
            ..Default::default()
        },
        strategy: ResolutionStrategy::AddModifier {
            modifier: "A-".to_string(),
        },
    });

    // Strategy 5: Individual remap for single-key conflicts
    if conflicting_keys.len() == 1 {
        let key = conflicting_keys[0];
        // Find an available nearby key
        let nearby = find_nearby_available_key(key, wf_a, wf_b);
        if let Some(new_key) = nearby {
            suggestions.push(ResolutionSuggestion {
                description: format!(
                    "Remap '{}' in '{}': '{key}' → '{new_key}'",
                    loser_id, new_key
                ),
                score: 85,
                override_rule: WorkflowOverride {
                    workflow: loser_id.to_string(),
                    remaps: Some(vec![KeyRemap {
                        from: key.to_string(),
                        to: new_key,
                    }]),
                    ..Default::default()
                },
                strategy: ResolutionStrategy::RemapKey {
                    from: key.to_string(),
                    to: "".to_string(),
                },
            });
        }
    }

    // Sort by score (highest first)
    suggestions.sort_by_key(|s| std::cmp::Reverse(s.score));
    suggestions
}

/// Find a nearby unbound key for single-key remap suggestions.
fn find_nearby_available_key(
    key: &str,
    wf_a: Option<&&WorkflowBindings>,
    wf_b: Option<&&WorkflowBindings>,
) -> Option<String> {
    let used: HashSet<String> = wf_a
        .into_iter()
        .chain(wf_b)
        .flat_map(|w| w.bindings.iter().map(|b| b.keys.clone()))
        .collect();

    // Keyboard-aware candidates: same finger, adjacent keys
    let candidates = match key {
        "h" => vec!["y", "u", "n", "b"],
        "j" => vec!["k", "n", "m", "u"],
        "k" => vec!["j", "l", "i", "m"],
        "l" => vec!["k", ";", "o", "p"],
        "g" => vec!["f", "t", "b", "v"],
        "s" => vec!["d", "a", "w", "x"],
        "m" => vec!["n", ",", "k", "j"],
        _ => vec!["z", "x", "'", ";", "\\"],
    };

    candidates
        .into_iter()
        .find(|c| !used.contains(*c))
        .map(String::from)
}

// ---------------------------------------------------------------------------
// Convenience
// ---------------------------------------------------------------------------

/// Quick conflict check between two workflows.
pub fn detect_conflicts(a: &WorkflowBindings, b: &WorkflowBindings) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    let keys_a: HashMap<(&str, Option<String>), &KeybindDef> = a
        .bindings
        .iter()
        .map(|b| {
            (
                (
                    b.keys.as_str(),
                    b.context.as_ref().map(|c| format!("{c:?}")),
                ),
                b,
            )
        })
        .collect();

    for b_bind in &b.bindings {
        let ctx = b_bind.context.as_ref().map(|c| format!("{c:?}"));
        if let Some(a_bind) = keys_a.get(&(b_bind.keys.as_str(), ctx.clone())) {
            conflicts.push(Conflict {
                keys: b_bind.keys.clone(),
                context: ctx,
                severity: ConflictSeverity::Hard,
                workflow_a: a.id.clone(),
                action_a: a_bind.action.clone(),
                desc_a: a_bind.desc.clone(),
                workflow_b: b.id.clone(),
                action_b: b_bind.action.clone(),
                desc_b: b_bind.desc.clone(),
            });
        }
    }
    conflicts
}
