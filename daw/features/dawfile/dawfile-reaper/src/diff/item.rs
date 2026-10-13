//! Item-level diffing — GUID-based matching with take/MIDI sub-diffs.

use std::collections::HashMap;

use super::f64_eq;
use super::midi;
use super::types::*;
use crate::types::Item;
use crate::types::item::Take;

/// Diff two lists of items, matched by `item_guid` (IGUID).
///
/// When `options.position_offset` is set, item positions in `new` are shifted
/// by `-offset` before comparison. When `options.time_window` is set, items
/// in `new` outside the window are excluded.
pub(crate) fn diff_items(old: &[Item], new: &[Item], options: &DiffOptions) -> Vec<ItemDiff> {
    let mut diffs = Vec::new();

    // Filter new items by time window (if set)
    let in_window = |item: &&Item| -> bool {
        if let Some((start, end)) = options.time_window {
            item.position >= start && item.position < end
        } else {
            true
        }
    };

    let by_name = options.match_items_by_name;
    let offset = options.position_offset;

    // Item key: GUID if available and not matching by name, otherwise name+offset-adjusted-position
    let item_key = |item: &Item, apply_offset: bool| -> String {
        if !by_name && let Some(ref guid) = item.item_guid {
            return guid.clone();
        }
        let pos = if apply_offset {
            item.position - offset
        } else {
            item.position
        };
        format!("{}@{:.3}", item.name, pos)
    };

    let old_map: HashMap<String, &Item> = old.iter().map(|i| (item_key(i, false), i)).collect();

    let new_map: HashMap<String, &Item> = new
        .iter()
        .filter(in_window)
        .map(|i| (item_key(i, true), i))
        .collect();

    // Remaining unmatched items (should be empty with good key generation)
    let _old_no_match: Vec<&Item> = Vec::new();
    let _new_no_match: Vec<&Item> = Vec::new();

    // Removed or modified
    for (key, &old_item) in &old_map {
        match new_map.get(key) {
            None => {
                diffs.push(ItemDiff {
                    guid: old_item.item_guid.clone(),
                    name: old_item.name.clone(),
                    kind: ChangeKind::Removed,
                    property_changes: Vec::new(),
                    takes: Vec::new(),
                });
            }
            Some(&new_item) => {
                if let Some(diff) = diff_single_item(old_item, new_item, options) {
                    diffs.push(diff);
                }
            }
        }
    }

    // Added
    for (key, &new_item) in &new_map {
        if !old_map.contains_key(key) {
            diffs.push(ItemDiff {
                guid: new_item.item_guid.clone(),
                name: new_item.name.clone(),
                kind: ChangeKind::Added,
                property_changes: Vec::new(),
                takes: Vec::new(),
            });
        }
    }

    diffs
}

fn diff_single_item(old: &Item, new: &Item, options: &DiffOptions) -> Option<ItemDiff> {
    let mut props = Vec::new();
    let offset = options.position_offset;

    macro_rules! check {
        ($field:ident) => {
            if old.$field != new.$field {
                props.push(PropertyChange {
                    field: stringify!($field).into(),
                    old_value: format!("{:?}", old.$field),
                    new_value: format!("{:?}", new.$field),
                });
            }
        };
    }

    // Position is compared with offset subtracted from new
    if !f64_eq(old.position, new.position - offset) {
        props.push(PropertyChange {
            field: "position".into(),
            old_value: format!("{:.6}", old.position),
            new_value: format!("{:.6}", new.position - offset),
        });
    }
    if !f64_eq(old.length, new.length) {
        props.push(PropertyChange {
            field: "length".into(),
            old_value: format!("{:.6}", old.length),
            new_value: format!("{:.6}", new.length),
        });
    }
    if !f64_eq(old.snap_offset, new.snap_offset) {
        props.push(PropertyChange {
            field: "snap_offset".into(),
            old_value: format!("{:.6}", old.snap_offset),
            new_value: format!("{:.6}", new.snap_offset),
        });
    }
    check!(name);
    check!(loop_source);
    check!(mute);
    check!(color);
    check!(selected);
    check!(channel_mode);
    check!(volpan);
    check!(fade_in);
    check!(fade_out);
    check!(playrate);

    let take_diffs = diff_takes(&old.takes, &new.takes);

    if props.is_empty() && take_diffs.is_empty() {
        None
    } else {
        Some(ItemDiff {
            guid: new.item_guid.clone(),
            name: new.name.clone(),
            kind: ChangeKind::Modified,
            property_changes: props,
            takes: take_diffs,
        })
    }
}

fn diff_takes(old: &[Take], new: &[Take]) -> Vec<TakeDiff> {
    let mut diffs = Vec::new();

    // Match by take GUID
    let old_map: HashMap<&str, &Take> = old
        .iter()
        .filter_map(|t| t.take_guid.as_deref().map(|g| (g, t)))
        .collect();

    let new_map: HashMap<&str, &Take> = new
        .iter()
        .filter_map(|t| t.take_guid.as_deref().map(|g| (g, t)))
        .collect();

    for (&guid, &old_take) in &old_map {
        match new_map.get(guid) {
            None => {
                diffs.push(TakeDiff {
                    guid: Some(guid.to_string()),
                    name: old_take.name.clone(),
                    kind: ChangeKind::Removed,
                    property_changes: Vec::new(),
                    midi: None,
                });
            }
            Some(&new_take) => {
                let mut props = Vec::new();
                if old_take.name != new_take.name {
                    props.push(PropertyChange {
                        field: "name".into(),
                        old_value: old_take.name.clone(),
                        new_value: new_take.name.clone(),
                    });
                }

                let midi_diff = midi::diff_midi_sources(
                    old_take.source.as_ref().and_then(|s| s.midi_data.as_ref()),
                    new_take.source.as_ref().and_then(|s| s.midi_data.as_ref()),
                );

                if !props.is_empty() || midi_diff.is_some() {
                    diffs.push(TakeDiff {
                        guid: Some(guid.to_string()),
                        name: new_take.name.clone(),
                        kind: ChangeKind::Modified,
                        property_changes: props,
                        midi: midi_diff,
                    });
                }
            }
        }
    }

    for (&guid, &new_take) in &new_map {
        if !old_map.contains_key(guid) {
            diffs.push(TakeDiff {
                guid: Some(guid.to_string()),
                name: new_take.name.clone(),
                kind: ChangeKind::Added,
                property_changes: Vec::new(),
                midi: None,
            });
        }
    }

    diffs
}
