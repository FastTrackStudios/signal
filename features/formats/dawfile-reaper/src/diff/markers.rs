//! Marker and region diffing.

use std::collections::HashMap;

use super::f64_eq;
use super::types::*;
use crate::types::MarkerRegionCollection;

pub(crate) fn diff_markers_regions(
    old: &MarkerRegionCollection,
    new: &MarkerRegionCollection,
    options: &DiffOptions,
) -> Vec<MarkerRegionDiff> {
    let offset = options.position_offset;
    let mut diffs = Vec::new();

    // Build lookup by (is_region, id) — the stable identity for markers/regions
    let old_map: HashMap<(bool, i32), _> = old
        .markers
        .iter()
        .map(|m| ((false, m.id), m))
        .chain(old.regions.iter().map(|r| ((true, r.id), r)))
        .collect();

    let new_map: HashMap<(bool, i32), _> = new
        .markers
        .iter()
        .map(|m| ((false, m.id), m))
        .chain(new.regions.iter().map(|r| ((true, r.id), r)))
        .collect();

    // Removed or modified
    for (&key, &old_entry) in &old_map {
        let (is_region, id) = key;
        match new_map.get(&key) {
            None => {
                diffs.push(MarkerRegionDiff {
                    id,
                    name: old_entry.name.clone(),
                    is_region,
                    kind: ChangeKind::Removed,
                    property_changes: Vec::new(),
                });
            }
            Some(&new_entry) => {
                let mut changes = Vec::new();
                if old_entry.name != new_entry.name {
                    changes.push(PropertyChange {
                        field: "name".into(),
                        old_value: old_entry.name.clone(),
                        new_value: new_entry.name.clone(),
                    });
                }
                if !f64_eq(old_entry.position, new_entry.position - offset) {
                    changes.push(PropertyChange {
                        field: "position".into(),
                        old_value: format!("{:.6}", old_entry.position),
                        new_value: format!("{:.6}", new_entry.position - offset),
                    });
                }
                if is_region && old_entry.end_position != new_entry.end_position {
                    changes.push(PropertyChange {
                        field: "end_position".into(),
                        old_value: old_entry
                            .end_position
                            .map_or("none".into(), |v| format!("{:.6}", v)),
                        new_value: new_entry
                            .end_position
                            .map_or("none".into(), |v| format!("{:.6}", v)),
                    });
                }
                if old_entry.color != new_entry.color {
                    changes.push(PropertyChange {
                        field: "color".into(),
                        old_value: format!("{}", old_entry.color),
                        new_value: format!("{}", new_entry.color),
                    });
                }
                if !changes.is_empty() {
                    diffs.push(MarkerRegionDiff {
                        id,
                        name: new_entry.name.clone(),
                        is_region,
                        kind: ChangeKind::Modified,
                        property_changes: changes,
                    });
                }
            }
        }
    }

    // Added
    for (&key, &new_entry) in &new_map {
        let (is_region, id) = key;
        if !old_map.contains_key(&key) {
            diffs.push(MarkerRegionDiff {
                id,
                name: new_entry.name.clone(),
                is_region,
                kind: ChangeKind::Added,
                property_changes: Vec::new(),
            });
        }
    }

    diffs.sort_by_key(|d| (d.is_region, d.id));
    diffs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MarkerRegion, MarkerRegionCollection};

    fn marker(id: i32, name: &str, pos: f64) -> MarkerRegion {
        MarkerRegion {
            id,
            name: name.into(),
            position: pos,
            end_position: None,
            color: 0,
            flags: 0,
            locked: 0,
            guid: String::new(),
            additional: 0,
            lane: None,
            beat_position: None,
        }
    }

    fn region(id: i32, name: &str, pos: f64, end: f64) -> MarkerRegion {
        MarkerRegion {
            id,
            name: name.into(),
            position: pos,
            end_position: Some(end),
            color: 0,
            flags: 0,
            locked: 0,
            guid: String::new(),
            additional: 0,
            lane: None,
            beat_position: None,
        }
    }

    fn collection(
        markers: Vec<MarkerRegion>,
        regions: Vec<MarkerRegion>,
    ) -> MarkerRegionCollection {
        let mut all = Vec::new();
        all.extend(markers.iter().cloned());
        all.extend(regions.iter().cloned());
        MarkerRegionCollection {
            all,
            markers,
            regions,
        }
    }

    #[test]
    fn no_changes() {
        let col = collection(
            vec![marker(1, "SONGSTART", 0.0)],
            vec![region(1, "Intro", 0.0, 8.0)],
        );
        let diffs = diff_markers_regions(&col, &col, &DiffOptions::default());
        assert!(diffs.is_empty());
    }

    #[test]
    fn marker_added() {
        let old = collection(vec![], vec![]);
        let new = collection(vec![marker(1, "SONGSTART", 0.0)], vec![]);
        let diffs = diff_markers_regions(&old, &new, &DiffOptions::default());
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].kind, ChangeKind::Added);
        assert_eq!(diffs[0].name, "SONGSTART");
    }

    #[test]
    fn marker_moved() {
        let old = collection(vec![marker(1, "SONGEND", 8.0)], vec![]);
        let new = collection(vec![marker(1, "SONGEND", 10.0)], vec![]);
        let diffs = diff_markers_regions(&old, &new, &DiffOptions::default());
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].kind, ChangeKind::Modified);
        assert_eq!(diffs[0].property_changes[0].field, "position");
    }

    #[test]
    fn region_removed() {
        let old = collection(vec![], vec![region(1, "Intro", 0.0, 8.0)]);
        let new = collection(vec![], vec![]);
        let diffs = diff_markers_regions(&old, &new, &DiffOptions::default());
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].kind, ChangeKind::Removed);
        assert!(diffs[0].is_region);
    }
}
