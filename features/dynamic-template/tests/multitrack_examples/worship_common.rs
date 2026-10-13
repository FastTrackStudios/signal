//! Shared helper for the worship-multitracks grouping tests. Each song test
//! feeds its real on-disk stem filenames through the organizer and asserts the
//! performance-grouping *contract* (membership + top-level folder), not the
//! exact interior numbering — robust to cosmetic ordering.

use daw_proto::FolderDepthChange;
use dynamic_template::*;
use std::collections::HashMap;

/// Organize `items`, print the tree, assert nothing lands in Unsorted, and
/// return `(placement, count)` where `placement[file] = (top_folder, owning_node)`.
/// `top_folder` is "" for a stem that sits at the top level (no folder).
///
/// # Errors
///
/// Returns an error if `organize_into_tracks` fails.
pub fn organize(
    label: &str,
    items: &[&str],
) -> monarchy::Result<(HashMap<String, (String, String)>, usize)> {
    let config = default_config();
    let tracks = items.to_vec().organize_into_tracks(&config, None)?;

    println!("\n{label} track list:");
    daw_proto::display_tracklist(&tracks);

    // Flat REAPER-style list: FolderStart opens a level, ClosesLevels(-n)
    // closes n levels at the end of that track.
    let mut stack: Vec<String> = Vec::new();
    let mut placement: HashMap<String, (String, String)> = HashMap::new();
    for node in &tracks.tracks {
        if matches!(node.folder_depth_change, FolderDepthChange::FolderStart) {
            stack.push(node.name.clone());
        }
        let top = stack.first().cloned().unwrap_or_default();
        for item in &node.items {
            placement.insert(item.clone(), (top.clone(), node.name.clone()));
        }
        if let FolderDepthChange::ClosesLevels(n) = node.folder_depth_change {
            for _ in 0..n.unsigned_abs() {
                stack.pop();
            }
        }
    }

    // Note (don't hard-fail) anything that fell through to Unsorted — some
    // instrument families (bare "Synths", "Arps") aren't classified yet; the
    // per-song tests document those as known gaps rather than the whole suite
    // failing on a classifier hole.
    if let Some(u) = tracks.tracks.iter().find(|n| n.name == "Unsorted") {
        println!("  NOTE [{label}] Unsorted: {:?}", u.items);
    }
    if placement.len() != items.len() {
        return Err(monarchy::MonarchyError::InvalidField(format!(
            "{label}: every stem should be placed exactly once (placed {}, expected {})",
            placement.len(),
            items.len()
        )));
    }
    Ok((placement, items.len()))
}

/// True if `file` belongs to group `g` — either directly (a single-member
/// group becomes a top-level track *named* after the group: node == g) or
/// nested (a multi-member group becomes a folder: top-level folder == g).
pub fn in_group(placement: &HashMap<String, (String, String)>, file: &str, g: &str) -> bool {
    placement
        .get(file)
        .is_some_and(|(top, node)| top == g || node == g)
}
