//! Track hierarchy types for building folder structures
//!
//! DAWs represent track folders using relative depth changes rather than
//! explicit parent-child relationships. This module provides types for
//! building and representing these hierarchies.

use facet::Facet;

/// Folder depth change for hierarchical track structures
///
/// DAWs represent folder hierarchies using relative depth changes:
/// - Normal tracks have no change (0)
/// - Folder starts increase depth (+1)
/// - Tracks can close one or more folder levels (negative values)
///
/// # Example
///
/// A structure like:
/// ```text
/// Drums (folder)
///   Kick (folder)
///     Kick In
///     Kick Out  <- closes Kick folder
///   Snare       <- closes Drums folder
/// ```
///
/// Would be represented as:
/// - Drums: FolderStart
/// - Kick: FolderStart
/// - Kick In: Normal
/// - Kick Out: ClosesLevels(-1)
/// - Snare: ClosesLevels(-1)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Facet)]
pub enum FolderDepthChange {
    /// Normal track (no folder change)
    #[default]
    Normal,
    /// Start of a folder (depth +1)
    FolderStart,
    /// Closes N folder levels (negative values indicate levels to close)
    ClosesLevels(i8),
}

impl FolderDepthChange {
    /// Convert to raw integer value for DAW APIs
    pub fn to_raw_value(self) -> i32 {
        match self {
            Self::Normal => 0,
            Self::FolderStart => 1,
            Self::ClosesLevels(n) => n as i32,
        }
    }

    /// Create from raw integer value from DAW APIs
    pub fn from_raw_value(value: i32) -> Self {
        match value {
            0 => Self::Normal,
            1 => Self::FolderStart,
            n if n < 0 => Self::ClosesLevels(n as i8),
            _ => Self::Normal,
        }
    }

    /// Check if this is a folder start
    pub fn is_folder_start(&self) -> bool {
        matches!(self, Self::FolderStart)
    }

    /// Check if this closes any folder levels
    pub fn closes_folders(&self) -> bool {
        matches!(self, Self::ClosesLevels(_))
    }

    /// Get the number of levels closed (0 if not closing)
    pub fn levels_closed(&self) -> u8 {
        match self {
            Self::ClosesLevels(n) => n.unsigned_abs(),
            _ => 0,
        }
    }
}

/// A node in a track hierarchy (for building/organizing)
///
/// This represents a single track in a hierarchy, with folder depth
/// information encoded as relative changes from the previous track.
#[derive(Clone, Debug, Facet)]
pub struct TrackNode {
    /// Display name for the track
    pub name: String,
    /// Whether this is a folder track
    pub is_folder: bool,
    /// Folder depth change relative to previous track
    pub folder_depth_change: FolderDepthChange,
    /// Items/media on this track (names or IDs)
    pub items: Vec<String>,
    /// Optional color (0xRRGGBB)
    pub color: Option<u32>,
    /// Optional metadata (JSON-serializable)
    pub metadata: Option<String>,
}

impl TrackNode {
    /// Create a new normal (non-folder) track
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            is_folder: false,
            folder_depth_change: FolderDepthChange::Normal,
            items: Vec::new(),
            color: None,
            metadata: None,
        }
    }

    /// Create a new folder track
    pub fn folder(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            is_folder: true,
            folder_depth_change: FolderDepthChange::FolderStart,
            items: Vec::new(),
            color: None,
            metadata: None,
        }
    }

    /// Add an item to this track
    pub fn with_item(mut self, item: impl Into<String>) -> Self {
        self.items.push(item.into());
        self
    }

    /// Add multiple items to this track
    pub fn with_items(mut self, items: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.items.extend(items.into_iter().map(|i| i.into()));
        self
    }

    /// Set the color
    pub fn with_color(mut self, color: u32) -> Self {
        self.color = Some(color);
        self
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: impl Into<String>) -> Self {
        self.metadata = Some(metadata.into());
        self
    }
}

impl Default for TrackNode {
    fn default() -> Self {
        Self::new("")
    }
}

/// A complete track hierarchy (flat list with folder depth markers)
///
/// This represents a complete track structure as a flat list where
/// folder relationships are encoded in the `folder_depth_change` of each node.
#[derive(Clone, Debug, Default, Facet)]
pub struct TrackHierarchy {
    /// The tracks in order, with folder depth encoded in each node
    pub tracks: Vec<TrackNode>,
}

impl TrackHierarchy {
    /// Create an empty hierarchy
    pub fn new() -> Self {
        Self { tracks: Vec::new() }
    }

    /// Create a hierarchy from a list of track nodes
    pub fn from_tracks(tracks: Vec<TrackNode>) -> Self {
        Self { tracks }
    }

    /// Get the number of tracks
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Check if the hierarchy is empty
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Iterate over the tracks
    pub fn iter(&self) -> impl Iterator<Item = &TrackNode> {
        self.tracks.iter()
    }

    /// Get a mutable iterator over the tracks
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut TrackNode> {
        self.tracks.iter_mut()
    }

    /// Print a tree representation (useful for debugging)
    pub fn print_tree(&self) -> String {
        let mut output = String::new();
        let mut depth = 0i32;

        for track in &self.tracks {
            // Handle folder closing from previous track
            if let FolderDepthChange::ClosesLevels(n) = track.folder_depth_change {
                depth = (depth + n as i32).max(0);
            }

            // Print indentation and track name
            let indent = "  ".repeat(depth as usize);
            let folder_marker = if track.is_folder { "[F] " } else { "" };
            output.push_str(&format!("{}{}{}\n", indent, folder_marker, track.name));

            // Handle folder start
            if track.folder_depth_change == FolderDepthChange::FolderStart {
                depth += 1;
            }
        }

        output
    }
}

impl IntoIterator for TrackHierarchy {
    type Item = TrackNode;
    type IntoIter = std::vec::IntoIter<TrackNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.tracks.into_iter()
    }
}

impl<'a> IntoIterator for &'a TrackHierarchy {
    type Item = &'a TrackNode;
    type IntoIter = std::slice::Iter<'a, TrackNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.tracks.iter()
    }
}

impl FromIterator<TrackNode> for TrackHierarchy {
    fn from_iter<T: IntoIterator<Item = TrackNode>>(iter: T) -> Self {
        Self {
            tracks: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_folder_depth_change_conversion() {
        assert_eq!(FolderDepthChange::Normal.to_raw_value(), 0);
        assert_eq!(FolderDepthChange::FolderStart.to_raw_value(), 1);
        assert_eq!(FolderDepthChange::ClosesLevels(-1).to_raw_value(), -1);
        assert_eq!(FolderDepthChange::ClosesLevels(-2).to_raw_value(), -2);

        assert_eq!(
            FolderDepthChange::from_raw_value(0),
            FolderDepthChange::Normal
        );
        assert_eq!(
            FolderDepthChange::from_raw_value(1),
            FolderDepthChange::FolderStart
        );
        assert_eq!(
            FolderDepthChange::from_raw_value(-1),
            FolderDepthChange::ClosesLevels(-1)
        );
        assert_eq!(
            FolderDepthChange::from_raw_value(-3),
            FolderDepthChange::ClosesLevels(-3)
        );
    }

    #[test]
    fn test_track_node_builder() {
        let node = TrackNode::new("Kick In")
            .with_item("kick_in.wav")
            .with_color(0xFF0000);

        assert_eq!(node.name, "Kick In");
        assert!(!node.is_folder);
        assert_eq!(node.items, vec!["kick_in.wav"]);
        assert_eq!(node.color, Some(0xFF0000));
    }

    #[test]
    fn test_folder_node() {
        let node = TrackNode::folder("Drums");

        assert_eq!(node.name, "Drums");
        assert!(node.is_folder);
        assert_eq!(node.folder_depth_change, FolderDepthChange::FolderStart);
    }
}
