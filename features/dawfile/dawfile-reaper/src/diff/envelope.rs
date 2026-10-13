//! Envelope diffing — position-based merge for points without GUIDs.

use std::collections::HashMap;

use super::f64_eq;
use super::types::*;
use crate::types::envelope::{AutomationItem, EnvelopePoint};
use crate::types::{Envelope, TempoTimeEnvelope, TempoTimePoint};

/// Diff two lists of envelopes, matched by `(guid, envelope_type)`.
pub(crate) fn diff_envelopes(
    old: &[Envelope],
    new: &[Envelope],
    options: &DiffOptions,
) -> Vec<EnvelopeDiff> {
    let mut diffs = Vec::new();

    let old_map: HashMap<(&str, &str), &Envelope> = old
        .iter()
        .map(|e| ((e.guid.as_str(), e.envelope_type.as_str()), e))
        .collect();

    let new_map: HashMap<(&str, &str), &Envelope> = new
        .iter()
        .map(|e| ((e.guid.as_str(), e.envelope_type.as_str()), e))
        .collect();

    for (&key, &old_env) in &old_map {
        match new_map.get(&key) {
            None => {
                diffs.push(EnvelopeDiff {
                    guid: old_env.guid.clone(),
                    envelope_type: old_env.envelope_type.clone(),
                    kind: ChangeKind::Removed,
                    property_changes: Vec::new(),
                    point_changes: Vec::new(),
                    automation_item_changes: Vec::new(),
                });
            }
            Some(&new_env) => {
                let prop_changes = diff_envelope_properties(old_env, new_env);
                let point_changes =
                    diff_points(&old_env.points, &new_env.points, options.position_offset);
                let ai_changes =
                    diff_automation_items(&old_env.automation_items, &new_env.automation_items);

                if !prop_changes.is_empty() || !point_changes.is_empty() || !ai_changes.is_empty() {
                    diffs.push(EnvelopeDiff {
                        guid: new_env.guid.clone(),
                        envelope_type: new_env.envelope_type.clone(),
                        kind: ChangeKind::Modified,
                        property_changes: prop_changes,
                        point_changes,
                        automation_item_changes: ai_changes,
                    });
                }
            }
        }
    }

    for (&key, &new_env) in &new_map {
        if !old_map.contains_key(&key) {
            diffs.push(EnvelopeDiff {
                guid: new_env.guid.clone(),
                envelope_type: new_env.envelope_type.clone(),
                kind: ChangeKind::Added,
                property_changes: Vec::new(),
                point_changes: Vec::new(),
                automation_item_changes: Vec::new(),
            });
        }
    }

    diffs
}

fn diff_envelope_properties(old: &Envelope, new: &Envelope) -> Vec<PropertyChange> {
    let mut changes = Vec::new();
    if old.active != new.active {
        changes.push(PropertyChange {
            field: "active".into(),
            old_value: old.active.to_string(),
            new_value: new.active.to_string(),
        });
    }
    if old.visible != new.visible {
        changes.push(PropertyChange {
            field: "visible".into(),
            old_value: old.visible.to_string(),
            new_value: new.visible.to_string(),
        });
    }
    if old.armed != new.armed {
        changes.push(PropertyChange {
            field: "armed".into(),
            old_value: old.armed.to_string(),
            new_value: new.armed.to_string(),
        });
    }
    changes
}

/// Position-based two-pointer merge for envelope points.
///
/// Both lists are assumed sorted by position. We advance through both lists:
/// - If positions match (within epsilon): compare value/shape → Modified or skip
/// - If old < new: old point was Removed
/// - If new < old: new point was Added
///
/// `offset` is subtracted from new point positions before comparison.
///
/// O(n + m) where n = old.len(), m = new.len().
pub(crate) fn diff_points(
    old: &[EnvelopePoint],
    new: &[EnvelopePoint],
    offset: f64,
) -> Vec<PointChange> {
    let mut changes = Vec::new();
    let mut i = 0;
    let mut j = 0;

    while i < old.len() && j < new.len() {
        let op = &old[i];
        let np = &new[j];
        let np_pos = np.position - offset; // Offset-adjusted position

        if f64_eq(op.position, np_pos) {
            // Same position — check if value or shape changed
            if !f64_eq(op.value, np.value) || op.shape != np.shape {
                changes.push(PointChange::Modified {
                    old: snapshot(op),
                    new: PointSnapshot {
                        position: np_pos,
                        value: np.value,
                        shape: np.shape as i32,
                    },
                });
            }
            i += 1;
            j += 1;
        } else if op.position < np_pos {
            changes.push(PointChange::Removed(snapshot(op)));
            i += 1;
        } else {
            changes.push(PointChange::Added(PointSnapshot {
                position: np_pos,
                value: np.value,
                shape: np.shape as i32,
            }));
            j += 1;
        }
    }

    // Drain remaining
    while i < old.len() {
        changes.push(PointChange::Removed(snapshot(&old[i])));
        i += 1;
    }
    while j < new.len() {
        let np = &new[j];
        changes.push(PointChange::Added(PointSnapshot {
            position: np.position - offset,
            value: np.value,
            shape: np.shape as i32,
        }));
        j += 1;
    }

    changes
}

fn snapshot(p: &EnvelopePoint) -> PointSnapshot {
    PointSnapshot {
        position: p.position,
        value: p.value,
        shape: p.shape as i32,
    }
}

fn diff_automation_items(
    old: &[AutomationItem],
    new: &[AutomationItem],
) -> Vec<AutomationItemDiff> {
    // Match by (pool_index, instance_index)
    let mut diffs = Vec::new();

    let old_map: HashMap<(i32, i32), &AutomationItem> = old
        .iter()
        .map(|ai| ((ai.pool_index, ai.instance_index), ai))
        .collect();

    let new_map: HashMap<(i32, i32), &AutomationItem> = new
        .iter()
        .map(|ai| ((ai.pool_index, ai.instance_index), ai))
        .collect();

    for (&key, &old_ai) in &old_map {
        match new_map.get(&key) {
            None => {
                diffs.push(AutomationItemDiff {
                    pool_index: key.0,
                    instance_index: key.1,
                    kind: ChangeKind::Removed,
                    property_changes: Vec::new(),
                });
            }
            Some(&new_ai) => {
                let mut changes = Vec::new();
                if !f64_eq(old_ai.position, new_ai.position) {
                    changes.push(PropertyChange {
                        field: "position".into(),
                        old_value: format!("{:.6}", old_ai.position),
                        new_value: format!("{:.6}", new_ai.position),
                    });
                }
                if !f64_eq(old_ai.length, new_ai.length) {
                    changes.push(PropertyChange {
                        field: "length".into(),
                        old_value: format!("{:.6}", old_ai.length),
                        new_value: format!("{:.6}", new_ai.length),
                    });
                }
                if !changes.is_empty() {
                    diffs.push(AutomationItemDiff {
                        pool_index: key.0,
                        instance_index: key.1,
                        kind: ChangeKind::Modified,
                        property_changes: changes,
                    });
                }
            }
        }
    }

    for &key in new_map.keys() {
        if !old_map.contains_key(&key) {
            diffs.push(AutomationItemDiff {
                pool_index: key.0,
                instance_index: key.1,
                kind: ChangeKind::Added,
                property_changes: Vec::new(),
            });
        }
    }

    diffs
}

/// Diff the project-level tempo envelope.
pub(crate) fn diff_tempo_envelope(
    old: Option<&TempoTimeEnvelope>,
    new: Option<&TempoTimeEnvelope>,
    options: &DiffOptions,
) -> Option<TempoEnvelopeDiff> {
    match (old, new) {
        (None, None) => None,
        (Some(_), None) => Some(TempoEnvelopeDiff {
            default_tempo_changed: None,
            default_time_sig_changed: None,
            point_changes: vec![], // Entire envelope removed
        }),
        (None, Some(_)) => Some(TempoEnvelopeDiff {
            default_tempo_changed: None,
            default_time_sig_changed: None,
            point_changes: vec![], // Entire envelope added
        }),
        (Some(old_env), Some(new_env)) => {
            let tempo_changed = if !f64_eq(old_env.default_tempo, new_env.default_tempo) {
                Some((old_env.default_tempo, new_env.default_tempo))
            } else {
                None
            };

            let ts_changed = if old_env.default_time_signature != new_env.default_time_signature {
                Some((
                    old_env.default_time_signature,
                    new_env.default_time_signature,
                ))
            } else {
                None
            };

            let point_changes =
                diff_tempo_points(&old_env.points, &new_env.points, options.position_offset);

            if tempo_changed.is_none() && ts_changed.is_none() && point_changes.is_empty() {
                None
            } else {
                Some(TempoEnvelopeDiff {
                    default_tempo_changed: tempo_changed,
                    default_time_sig_changed: ts_changed,
                    point_changes,
                })
            }
        }
    }
}

fn diff_tempo_points(
    old: &[TempoTimePoint],
    new: &[TempoTimePoint],
    offset: f64,
) -> Vec<TempoPointChange> {
    let mut changes = Vec::new();
    let mut i = 0;
    let mut j = 0;

    while i < old.len() && j < new.len() {
        let op = &old[i];
        let np = &new[j];
        let np_pos = np.position - offset;

        if f64_eq(op.position, np_pos) {
            if !f64_eq(op.tempo, np.tempo) {
                changes.push(TempoPointChange::Modified {
                    position: op.position,
                    old_tempo: op.tempo,
                    new_tempo: np.tempo,
                });
            }
            i += 1;
            j += 1;
        } else if op.position < np_pos {
            changes.push(TempoPointChange::Removed {
                position: op.position,
                tempo: op.tempo,
                time_sig: op.time_signature(),
            });
            i += 1;
        } else {
            changes.push(TempoPointChange::Added {
                position: np_pos,
                tempo: np.tempo,
                time_sig: np.time_signature(),
            });
            j += 1;
        }
    }

    while i < old.len() {
        changes.push(TempoPointChange::Removed {
            position: old[i].position,
            tempo: old[i].tempo,
            time_sig: old[i].time_signature(),
        });
        i += 1;
    }
    while j < new.len() {
        changes.push(TempoPointChange::Added {
            position: new[j].position - offset,
            tempo: new[j].tempo,
            time_sig: new[j].time_signature(),
        });
        j += 1;
    }

    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::envelope::{EnvelopePoint, EnvelopePointShape};

    fn pt(pos: f64, val: f64) -> EnvelopePoint {
        EnvelopePoint {
            position: pos,
            value: val,
            shape: EnvelopePointShape::Linear,
            time_sig: None,
            selected: None,
            unknown_field_6: None,
            bezier_tension: None,
        }
    }

    #[test]
    fn identical_points_no_diff() {
        let points = vec![pt(0.0, 0.5), pt(1.0, 0.8), pt(2.0, 0.3)];
        let changes = diff_points(&points, &points, 0.0);
        assert!(changes.is_empty());
    }

    #[test]
    fn point_added_in_middle() {
        let old = vec![pt(0.0, 0.5), pt(2.0, 0.3)];
        let new = vec![pt(0.0, 0.5), pt(1.0, 0.8), pt(2.0, 0.3)];
        let changes = diff_points(&old, &new, 0.0);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], PointChange::Added(p) if f64_eq(p.position, 1.0)));
    }

    #[test]
    fn point_removed() {
        let old = vec![pt(0.0, 0.5), pt(1.0, 0.8), pt(2.0, 0.3)];
        let new = vec![pt(0.0, 0.5), pt(2.0, 0.3)];
        let changes = diff_points(&old, &new, 0.0);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], PointChange::Removed(p) if f64_eq(p.position, 1.0)));
    }

    #[test]
    fn point_value_changed() {
        let old = vec![pt(0.0, 0.5), pt(1.0, 0.8)];
        let new = vec![pt(0.0, 0.5), pt(1.0, 0.6)];
        let changes = diff_points(&old, &new, 0.0);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], PointChange::Modified { old, new }
            if f64_eq(old.value, 0.8) && f64_eq(new.value, 0.6)));
    }
}
