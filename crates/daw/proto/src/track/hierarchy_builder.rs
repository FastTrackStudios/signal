//! Fluent builder for track hierarchies
//!
//! Provides a convenient API for building nested folder structures
//! with automatic depth tracking and folder closing.

use super::{FolderDepthChange, TrackHierarchy, TrackNode};

/// Fluent builder for creating track hierarchies
///
/// Provides a convenient API for building nested folder structures
/// with automatic depth tracking and folder closing.
///
/// # Example - Chainable API
///
/// ```
/// use daw_proto::TrackHierarchyBuilder;
///
/// let hierarchy = TrackHierarchyBuilder::new()
///     .folder("Drums")
///         .folder("Kick")
///             .track("Kick In").item("kick_in.wav")
///             .track("Kick Out").item("kick_out.wav")
///         .end()
///         .track("Snare").item("snare.wav")
///     .end()
///     .build();
/// ```
pub struct TrackHierarchyBuilder {
    tracks: Vec<TrackNode>,
    folder_stack: Vec<String>,
}

impl TrackHierarchyBuilder {
    /// Create a new empty builder
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            folder_stack: Vec::new(),
        }
    }

    /// Add a folder track and enter it
    ///
    /// Subsequent calls to `track()` or `folder()` will be children of this folder
    /// until `end()` is called.
    pub fn folder(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        let node = TrackNode::folder(name.clone());
        self.tracks.push(node);
        self.folder_stack.push(name);
        self
    }

    /// Add a folder track with a specific color
    pub fn folder_with_color(mut self, name: impl Into<String>, color: u32) -> Self {
        let name = name.into();
        let node = TrackNode::folder(name.clone()).with_color(color);
        self.tracks.push(node);
        self.folder_stack.push(name);
        self
    }

    /// Add a normal (non-folder) track at the current nesting level
    ///
    /// Use `.item("name")` after this to add items to the track.
    ///
    /// # Example
    /// ```
    /// # use daw_proto::TrackHierarchyBuilder;
    /// let hierarchy = TrackHierarchyBuilder::new()
    ///     .track("Kick").item("kick.wav")
    ///     .track("Snare").item("snare.wav")
    ///     .build();
    /// ```
    pub fn track(mut self, name: impl Into<String>) -> Self {
        let node = TrackNode::new(name);
        self.tracks.push(node);
        self
    }

    /// Add an item to the last track
    ///
    /// This is called after `.track()` to add media items.
    /// Can be called multiple times to add multiple items.
    ///
    /// # Example
    /// ```
    /// # use daw_proto::TrackHierarchyBuilder;
    /// let hierarchy = TrackHierarchyBuilder::new()
    ///     .track("Multi-mic Kick")
    ///         .item("kick_in.wav")
    ///         .item("kick_out.wav")
    ///     .build();
    /// ```
    pub fn item(mut self, item: impl Into<String>) -> Self {
        if let Some(last_track) = self.tracks.last_mut() {
            last_track.items.push(item.into());
        }
        self
    }

    /// Add a track with a specific color
    pub fn track_with_color(mut self, name: impl Into<String>, color: u32) -> Self {
        let node = TrackNode::new(name).with_color(color);
        self.tracks.push(node);
        self
    }

    /// Add a pre-built track node
    pub fn node(mut self, node: TrackNode) -> Self {
        let is_folder_start =
            node.is_folder && node.folder_depth_change == FolderDepthChange::FolderStart;
        let name = node.name.clone();
        self.tracks.push(node);
        if is_folder_start {
            self.folder_stack.push(name);
        }
        self
    }

    /// Add a TrackGroup to the hierarchy
    ///
    /// This allows composing pre-built track groups into a hierarchy.
    /// The group's tracks are appended at the current level.
    pub fn group(mut self, group: super::test_utils::TrackGroup) -> Self {
        self.tracks.extend(group.into_tracks());
        self
    }

    /// Close the current folder level and return to the parent level
    ///
    /// This modifies the last track to close the folder.
    pub fn end(mut self) -> Self {
        if self.folder_stack.pop().is_some()
            && let Some(last) = self.tracks.last_mut()
        {
            let current = last.folder_depth_change.to_raw_value();
            last.folder_depth_change = FolderDepthChange::from_raw_value(current - 1);
        }
        self
    }

    /// Get the current folder depth
    pub fn current_depth(&self) -> usize {
        self.folder_stack.len()
    }

    /// Check if we're currently inside a folder
    pub fn in_folder(&self) -> bool {
        !self.folder_stack.is_empty()
    }

    /// Build the final hierarchy
    ///
    /// Automatically closes any remaining open folders.
    pub fn build(mut self) -> TrackHierarchy {
        // Auto-close any remaining open folders
        while !self.folder_stack.is_empty() {
            self = self.end();
        }
        TrackHierarchy::from_tracks(self.tracks)
    }
}

impl Default for TrackHierarchyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension trait for adding children to a track hierarchy
pub trait AddChildren {
    /// Add child tracks to a folder, automatically handling depth changes
    fn add_children(self, children: Vec<TrackNode>) -> Vec<TrackNode>;
}

impl AddChildren for TrackNode {
    fn add_children(mut self, children: Vec<TrackNode>) -> Vec<TrackNode> {
        if children.is_empty() {
            return vec![self];
        }

        // Make this a folder
        self.is_folder = true;
        self.folder_depth_change = FolderDepthChange::FolderStart;

        let mut result = vec![self];

        // Add children, modifying the last one to close the folder
        let child_count = children.len();
        for (i, mut child) in children.into_iter().enumerate() {
            if i == child_count - 1 {
                // Last child closes this folder
                let current = child.folder_depth_change.to_raw_value();
                child.folder_depth_change = FolderDepthChange::from_raw_value(current - 1);
            }
            result.push(child);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_hierarchy() {
        let hierarchy = TrackHierarchyBuilder::new()
            .folder("Drums")
            .track("Kick")
            .item("kick.wav")
            .track("Snare")
            .item("snare.wav")
            .end()
            .build();

        assert_eq!(hierarchy.len(), 3);

        // Drums is a folder start
        assert_eq!(hierarchy.tracks[0].name, "Drums");
        assert!(hierarchy.tracks[0].is_folder);
        assert_eq!(
            hierarchy.tracks[0].folder_depth_change,
            FolderDepthChange::FolderStart
        );

        // Kick is normal
        assert_eq!(hierarchy.tracks[1].name, "Kick");
        assert_eq!(hierarchy.tracks[1].items, vec!["kick.wav"]);
        assert_eq!(
            hierarchy.tracks[1].folder_depth_change,
            FolderDepthChange::Normal
        );

        // Snare closes the folder
        assert_eq!(hierarchy.tracks[2].name, "Snare");
        assert_eq!(hierarchy.tracks[2].items, vec!["snare.wav"]);
        assert_eq!(
            hierarchy.tracks[2].folder_depth_change,
            FolderDepthChange::ClosesLevels(-1)
        );
    }

    #[test]
    fn test_nested_hierarchy() {
        let hierarchy = TrackHierarchyBuilder::new()
            .folder("Drums")
            .folder("Kick")
            .track("Kick In")
            .item("kick_in.wav")
            .track("Kick Out")
            .item("kick_out.wav")
            .end()
            .folder("Snare")
            .track("Snare Top")
            .item("snare_top.wav")
            .track("Snare Bottom")
            .item("snare_bottom.wav")
            .end()
            .end()
            .build();

        assert_eq!(hierarchy.len(), 7);

        // Drums folder
        assert_eq!(hierarchy.tracks[0].name, "Drums");
        assert_eq!(
            hierarchy.tracks[0].folder_depth_change,
            FolderDepthChange::FolderStart
        );

        // Kick folder
        assert_eq!(hierarchy.tracks[1].name, "Kick");
        assert_eq!(
            hierarchy.tracks[1].folder_depth_change,
            FolderDepthChange::FolderStart
        );

        // Kick In
        assert_eq!(hierarchy.tracks[2].name, "Kick In");
        assert_eq!(
            hierarchy.tracks[2].folder_depth_change,
            FolderDepthChange::Normal
        );

        // Kick Out closes Kick folder
        assert_eq!(hierarchy.tracks[3].name, "Kick Out");
        assert_eq!(
            hierarchy.tracks[3].folder_depth_change,
            FolderDepthChange::ClosesLevels(-1)
        );

        // Snare folder
        assert_eq!(hierarchy.tracks[4].name, "Snare");
        assert_eq!(
            hierarchy.tracks[4].folder_depth_change,
            FolderDepthChange::FolderStart
        );

        // Snare Top
        assert_eq!(hierarchy.tracks[5].name, "Snare Top");
        assert_eq!(
            hierarchy.tracks[5].folder_depth_change,
            FolderDepthChange::Normal
        );

        // Snare Bottom closes both Snare and Drums folders
        assert_eq!(hierarchy.tracks[6].name, "Snare Bottom");
        assert_eq!(
            hierarchy.tracks[6].folder_depth_change,
            FolderDepthChange::ClosesLevels(-2)
        );
    }

    #[test]
    fn test_auto_close() {
        // Forgetting to call .end() should auto-close
        let hierarchy = TrackHierarchyBuilder::new()
            .folder("Drums")
            .track("Kick")
            .item("kick.wav")
            .build();

        assert_eq!(hierarchy.len(), 2);

        // Kick should close the Drums folder
        assert_eq!(
            hierarchy.tracks[1].folder_depth_change,
            FolderDepthChange::ClosesLevels(-1)
        );
    }

    #[test]
    fn test_multiple_items() {
        let hierarchy = TrackHierarchyBuilder::new()
            .track("Multi-mic Kick")
            .item("kick_in.wav")
            .item("kick_out.wav")
            .item("kick_sub.wav")
            .build();

        assert_eq!(hierarchy.len(), 1);
        assert_eq!(hierarchy.tracks[0].name, "Multi-mic Kick");
        assert_eq!(
            hierarchy.tracks[0].items,
            vec!["kick_in.wav", "kick_out.wav", "kick_sub.wav"]
        );
    }

    #[test]
    fn test_print_tree() {
        let hierarchy = TrackHierarchyBuilder::new()
            .folder("Drums")
            .folder("Kick")
            .track("Kick In")
            .item("kick_in.wav")
            .track("Kick Out")
            .item("kick_out.wav")
            .end()
            .track("Snare")
            .item("snare.wav")
            .end()
            .build();

        let tree = hierarchy.print_tree();
        println!("{}", tree);

        assert!(tree.contains("[F] Drums"));
        assert!(tree.contains("[F] Kick"));
        assert!(tree.contains("Kick In"));
        assert!(tree.contains("Snare"));
    }
}
