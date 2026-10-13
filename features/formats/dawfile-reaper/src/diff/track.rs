//! Track-level diffing — GUID or name-based matching with property comparison.

use std::collections::HashMap;

use super::f64_eq;
use super::types::*;
use super::{envelope, fx, item};
use crate::types::Track;

/// Track identity key — either GUID or name, depending on options.
fn track_key(track: &Track, by_name: bool) -> Option<&str> {
    if by_name {
        Some(track.name.as_str())
    } else {
        track.track_id.as_deref()
    }
}

/// Diff two lists of tracks.
///
/// When `options.match_tracks_by_name` is true, tracks are matched by name
/// instead of GUID. This is needed when diffing against a concatenated setlist
/// where track GUIDs were cleared during generation.
pub(crate) fn diff_tracks(old: &[Track], new: &[Track], options: &DiffOptions) -> Vec<TrackDiff> {
    let mut diffs = Vec::new();
    let by_name = options.match_tracks_by_name;

    // Build lookup maps using the appropriate key
    let old_map: HashMap<&str, &Track> = old
        .iter()
        .filter_map(|t| track_key(t, by_name).map(|k| (k, t)))
        .collect();

    let new_map: HashMap<&str, &Track> = new
        .iter()
        .filter_map(|t| track_key(t, by_name).map(|k| (k, t)))
        .collect();

    // Removed or modified
    for (&key, &old_track) in &old_map {
        match new_map.get(key) {
            None => {
                diffs.push(TrackDiff {
                    guid: old_track.track_id.clone(),
                    name: old_track.name.clone(),
                    kind: ChangeKind::Removed,
                    property_changes: Vec::new(),
                    items: Vec::new(),
                    envelopes: Vec::new(),
                    fx_chain: None,
                });
            }
            Some(&new_track) => {
                if let Some(diff) = diff_single_track(old_track, new_track, options) {
                    diffs.push(diff);
                }
            }
        }
    }

    // Added
    for (&key, &new_track) in &new_map {
        if !old_map.contains_key(key) {
            diffs.push(TrackDiff {
                guid: new_track.track_id.clone(),
                name: new_track.name.clone(),
                kind: ChangeKind::Added,
                property_changes: Vec::new(),
                items: Vec::new(),
                envelopes: Vec::new(),
                fx_chain: None,
            });
        }
    }

    diffs
}

fn diff_single_track(old: &Track, new: &Track, options: &DiffOptions) -> Option<TrackDiff> {
    let mut props = Vec::new();

    // Compare scalar properties (skip name when matching by name — it's the key)
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

    if !options.match_tracks_by_name {
        check!(name);
    }
    check!(selected);
    check!(locked);
    check!(peak_color);
    check!(automation_mode);
    check!(volpan);
    check!(mutesolo);
    check!(invert_phase);
    check!(folder);
    check!(channel_count);
    check!(fx_enabled);
    check!(record);
    check!(track_height);
    check!(master_send);

    // Sub-diffs
    let item_diffs = item::diff_items(&old.items, &new.items, options);
    let env_diffs = envelope::diff_envelopes(&old.envelopes, &new.envelopes, options);
    let fx_diff = fx::diff_fx_chains(old.fx_chain.as_ref(), new.fx_chain.as_ref());

    if props.is_empty() && item_diffs.is_empty() && env_diffs.is_empty() && fx_diff.is_none() {
        None
    } else {
        Some(TrackDiff {
            guid: new.track_id.clone(),
            name: new.name.clone(),
            kind: ChangeKind::Modified,
            property_changes: props,
            items: item_diffs,
            envelopes: env_diffs,
            fx_chain: fx_diff,
        })
    }
}
