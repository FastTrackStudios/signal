//! Test utilities for track hierarchies
//!
//! Provides utilities for testing and debugging track structures,
//! including display functions and assertion helpers.

use super::{FolderDepthChange, TrackHierarchy, TrackNode};
use std::fmt::Write;

/// Display a track hierarchy as a formatted tree
///
/// # Example output
/// ```text
/// [F] Drums
///   [F] Kick
///     In [Kick In]
///     Out [Kick Out]
///   Snare [Snare Top]
/// ```
pub fn display_tracklist(hierarchy: &TrackHierarchy) {
    println!("{}", format_tracklist(hierarchy));
}

/// Format a track hierarchy as a tree string
pub fn format_tracklist(hierarchy: &TrackHierarchy) -> String {
    let mut output = String::new();
    let mut depth = 0usize;

    for track in &hierarchy.tracks {
        let indent = "  ".repeat(depth);
        let folder_marker = if track.is_folder { "[F] " } else { "" };
        let items_str = if track.items.is_empty() {
            String::new()
        } else if track.items.len() == 1 {
            format!(" [{}]", track.items[0])
        } else {
            format!(" [{} items]", track.items.len())
        };

        writeln!(
            output,
            "{}{}{}{}",
            indent, folder_marker, track.name, items_str
        )
        .unwrap();

        // Update depth based on folder changes
        match track.folder_depth_change {
            FolderDepthChange::FolderStart => depth += 1,
            FolderDepthChange::ClosesLevels(n) => {
                depth = (depth as i32 + n as i32).max(0) as usize;
            }
            FolderDepthChange::Normal => {}
        }
    }

    output
}

/// Assert that two track hierarchies are structurally equal
///
/// Compares track names, folder structure, and items. Returns an error
/// with a detailed diff if the hierarchies don't match.
pub fn assert_tracks_equal(
    actual: &TrackHierarchy,
    expected: &TrackHierarchy,
) -> Result<(), Box<dyn std::error::Error>> {
    if actual.len() != expected.len() {
        return Err(format!(
            "Track count mismatch: got {}, expected {}\n\nActual:\n{}\nExpected:\n{}",
            actual.len(),
            expected.len(),
            format_tracklist(actual),
            format_tracklist(expected)
        )
        .into());
    }

    for (i, (actual_track, expected_track)) in
        actual.tracks.iter().zip(expected.tracks.iter()).enumerate()
    {
        if let Err(e) = compare_tracks(actual_track, expected_track, i) {
            return Err(format!(
                "{}\n\nActual:\n{}\nExpected:\n{}",
                e,
                format_tracklist(actual),
                format_tracklist(expected)
            )
            .into());
        }
    }

    Ok(())
}

fn compare_tracks(actual: &TrackNode, expected: &TrackNode, index: usize) -> Result<(), String> {
    if actual.name != expected.name {
        return Err(format!(
            "Track {} name mismatch: got '{}', expected '{}'",
            index, actual.name, expected.name
        ));
    }

    if actual.is_folder != expected.is_folder {
        return Err(format!(
            "Track {} '{}' folder mismatch: got {}, expected {}",
            index, actual.name, actual.is_folder, expected.is_folder
        ));
    }

    if actual.folder_depth_change != expected.folder_depth_change {
        return Err(format!(
            "Track {} '{}' folder_depth_change mismatch: got {:?}, expected {:?}",
            index, actual.name, actual.folder_depth_change, expected.folder_depth_change
        ));
    }

    if actual.items != expected.items {
        return Err(format!(
            "Track {} '{}' items mismatch: got {:?}, expected {:?}",
            index, actual.name, actual.items, expected.items
        ));
    }

    Ok(())
}

/// Builder alias for backwards compatibility with tests using TrackStructureBuilder
pub type TrackStructureBuilder = super::TrackHierarchyBuilder;

/// A group of tracks that can be composed into a hierarchy
///
/// TrackGroup provides a fluent API for creating track groups (folders with children)
/// that can then be composed using TrackStructureBuilder::group().
///
/// # Example
/// ```
/// use daw_proto::{TrackGroup, TrackStructureBuilder};
///
/// let drums = TrackGroup::folder("Drums")
///     .track("Kick").item("kick.wav")
///     .track("Snare").item("snare.wav")
///     .end();
///
/// let bass = TrackGroup::single_track("Bass", "bass.wav");
///
/// let expected = TrackStructureBuilder::new()
///     .group(drums)
///     .group(bass)
///     .build();
/// ```
pub struct TrackGroup {
    tracks: Vec<TrackNode>,
}

impl TrackGroup {
    /// Create a folder group (folder with child tracks)
    pub fn folder(name: impl Into<String>) -> TrackGroupBuilder {
        TrackGroupBuilder {
            folder_name: name.into(),
            tracks: Vec::new(),
        }
    }

    /// Create a single track (no folder)
    pub fn single_track(name: impl Into<String>, item: impl Into<String>) -> Self {
        let mut track = TrackNode::new(name);
        track.items.push(item.into());
        Self {
            tracks: vec![track],
        }
    }

    /// Create a single track without items
    pub fn track(name: impl Into<String>) -> Self {
        Self {
            tracks: vec![TrackNode::new(name)],
        }
    }

    /// Get the tracks in this group
    pub fn into_tracks(self) -> Vec<TrackNode> {
        self.tracks
    }
}

/// Builder for creating TrackGroup
pub struct TrackGroupBuilder {
    folder_name: String,
    tracks: Vec<TrackNode>,
}

impl TrackGroupBuilder {
    /// Add a track to this group
    ///
    /// Use `.item()` after this to add items to the track.
    pub fn track(mut self, name: impl Into<String>) -> Self {
        self.tracks.push(TrackNode::new(name));
        self
    }

    /// Add an item to the last track in this group
    pub fn item(mut self, item: impl Into<String>) -> Self {
        if let Some(last_track) = self.tracks.last_mut() {
            last_track.items.push(item.into());
        }
        self
    }

    /// Add a nested folder within this group
    pub fn folder(self, name: impl Into<String>) -> NestedFolderBuilder {
        NestedFolderBuilder {
            parent: self,
            folder_name: name.into(),
            tracks: Vec::new(),
        }
    }

    /// Add a pre-built TrackGroup as a nested group
    pub fn group(mut self, group: TrackGroup) -> Self {
        self.tracks.extend(group.into_tracks());
        self
    }

    /// Finish building this group
    pub fn end(self) -> TrackGroup {
        let mut result = Vec::new();

        // Create folder node
        let folder = TrackNode::folder(self.folder_name);
        result.push(folder);

        // Add child tracks, marking the last one to close the folder
        let child_count = self.tracks.len();
        for (i, mut track) in self.tracks.into_iter().enumerate() {
            if i == child_count - 1 {
                // Last child closes this folder
                let current = track.folder_depth_change.to_raw_value();
                track.folder_depth_change = FolderDepthChange::from_raw_value(current - 1);
            }
            result.push(track);
        }

        // Handle empty folder case
        if child_count == 0
            && let Some(folder) = result.first_mut()
        {
            folder.folder_depth_change = FolderDepthChange::ClosesLevels(-1);
            folder.is_folder = false; // Empty folder becomes normal track
        }

        TrackGroup { tracks: result }
    }
}

/// Builder for nested folders within a TrackGroup
pub struct NestedFolderBuilder {
    parent: TrackGroupBuilder,
    folder_name: String,
    tracks: Vec<TrackNode>,
}

impl NestedFolderBuilder {
    /// Add a track to this nested folder
    ///
    /// Use `.item()` after this to add items to the track.
    pub fn track(mut self, name: impl Into<String>) -> Self {
        self.tracks.push(TrackNode::new(name));
        self
    }

    /// Add an item to the last track in this nested folder
    pub fn item(mut self, item: impl Into<String>) -> Self {
        if let Some(last_track) = self.tracks.last_mut() {
            last_track.items.push(item.into());
        }
        self
    }

    /// Finish this nested folder and return to the parent builder
    pub fn end(mut self) -> TrackGroupBuilder {
        // Create nested folder node
        let mut folder = TrackNode::folder(self.folder_name);

        // Add child tracks, marking the last one to close this nested folder
        let child_count = self.tracks.len();

        if child_count == 0 {
            // Empty nested folder - just add it as a track
            folder.is_folder = false;
            self.parent.tracks.push(folder);
        } else {
            self.parent.tracks.push(folder);
            for (i, mut track) in self.tracks.into_iter().enumerate() {
                if i == child_count - 1 {
                    let current = track.folder_depth_change.to_raw_value();
                    track.folder_depth_change = FolderDepthChange::from_raw_value(current - 1);
                }
                self.parent.tracks.push(track);
            }
        }

        self.parent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TrackHierarchyBuilder;

    #[test]
    fn test_format_tracklist() {
        let hierarchy = TrackHierarchyBuilder::new()
            .folder("Drums")
            .folder("Kick")
            .track("In")
            .item("Kick In")
            .track("Out")
            .item("Kick Out")
            .end()
            .track("Snare")
            .item("Snare Top")
            .end()
            .build();

        let output = format_tracklist(&hierarchy);
        println!("{}", output);

        assert!(output.contains("[F] Drums"));
        assert!(output.contains("[F] Kick"));
        assert!(output.contains("In [Kick In]"));
        assert!(output.contains("Out [Kick Out]"));
        assert!(output.contains("Snare [Snare Top]"));
    }

    #[test]
    fn test_assert_tracks_equal_success() {
        let hierarchy1 = TrackHierarchyBuilder::new()
            .folder("Drums")
            .track("Kick")
            .item("Kick In")
            .end()
            .build();

        let hierarchy2 = TrackHierarchyBuilder::new()
            .folder("Drums")
            .track("Kick")
            .item("Kick In")
            .end()
            .build();

        assert!(assert_tracks_equal(&hierarchy1, &hierarchy2).is_ok());
    }

    #[test]
    fn test_assert_tracks_equal_failure() {
        let hierarchy1 = TrackHierarchyBuilder::new()
            .folder("Drums")
            .track("Kick")
            .item("Kick In")
            .end()
            .build();

        let hierarchy2 = TrackHierarchyBuilder::new()
            .folder("Drums")
            .track("Snare")
            .item("Snare Top")
            .end()
            .build();

        assert!(assert_tracks_equal(&hierarchy1, &hierarchy2).is_err());
    }
}
