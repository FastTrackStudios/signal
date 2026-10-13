//! Track layouts based on instrument classification
//!
//! This module provides track layout name definitions for instrument groups that can be used
//! by REAPER extensions to automatically assign TCP and MCP layouts based on classification.
//!
//! REAPER track layouts are defined in themes. Common built-in layouts include:
//! - Default (standard layout with all controls)
//! - Small (compact layout)
//! - Folder (optimized for folder tracks)
//! - Aux/Bus (for bus/aux tracks)
//!
//! Custom themes may define additional layouts.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Layout name type
pub type LayoutName = &'static str;

/// Track layout configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackLayout {
    /// Layout for Track Control Panel (arrangement view)
    pub tcp: LayoutName,
    /// Layout for Mixer Control Panel
    pub mcp: LayoutName,
}

impl TrackLayout {
    #[must_use]
    pub const fn new(tcp: LayoutName, mcp: LayoutName) -> Self {
        Self { tcp, mcp }
    }

    #[must_use]
    pub const fn same(layout: LayoutName) -> Self {
        Self {
            tcp: layout,
            mcp: layout,
        }
    }
}

/// Standard layout names (common across most themes)
pub mod standard {
    use super::LayoutName;

    pub const DEFAULT: LayoutName = "";
    pub const SMALL: LayoutName = "Small";
    pub const FOLDER: LayoutName = "Folder";
    pub const BUS: LayoutName = "Bus";
    pub const AUX: LayoutName = "Aux";
    pub const SUBMIX: LayoutName = "Submix";
    pub const MASTER: LayoutName = "Master";
}

/// Layout definitions for top-level instrument groups
pub mod groups {
    use super::TrackLayout;

    // Primary instrument groups - use default layout for most
    pub const DRUMS: TrackLayout = TrackLayout::same("");
    pub const PERCUSSION: TrackLayout = TrackLayout::same("");
    pub const BASS: TrackLayout = TrackLayout::same("");
    pub const GUITARS: TrackLayout = TrackLayout::same("");
    pub const KEYS: TrackLayout = TrackLayout::same("");
    pub const SYNTHS: TrackLayout = TrackLayout::same("");
    pub const HORNS: TrackLayout = TrackLayout::same("");
    pub const HARMONICA: TrackLayout = TrackLayout::same("");
    pub const VOCALS: TrackLayout = TrackLayout::same("");
    pub const CHOIR: TrackLayout = TrackLayout::same("");
    pub const ORCHESTRA: TrackLayout = TrackLayout::same("");
    pub const SFX: TrackLayout = TrackLayout::same("");
    pub const GUIDE: TrackLayout = TrackLayout::same("Small");
    pub const REFERENCE: TrackLayout = TrackLayout::same("Small");
}

/// Static layout lookup table for hierarchical group paths
static LAYOUT_MAP: LazyLock<HashMap<&'static str, TrackLayout>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Top-level groups (lowercase for case-insensitive matching)
    m.insert("drums", groups::DRUMS);
    m.insert("percussion", groups::PERCUSSION);
    m.insert("bass", groups::BASS);
    m.insert("guitars", groups::GUITARS);
    m.insert("keys", groups::KEYS);
    m.insert("synths", groups::SYNTHS);
    m.insert("horns", groups::HORNS);
    m.insert("harmonica", groups::HARMONICA);
    m.insert("vocals", groups::VOCALS);
    m.insert("choir", groups::CHOIR);
    m.insert("orchestra", groups::ORCHESTRA);
    m.insert("sfx", groups::SFX);
    m.insert("guide", groups::GUIDE);
    m.insert("reference", groups::REFERENCE);

    // Guide/Click track types - small layout
    m.insert("click", TrackLayout::same("Small"));
    m.insert("click track", TrackLayout::same("Small"));
    m.insert("guide track", TrackLayout::same("Small"));
    m.insert("cue", TrackLayout::same("Small"));
    m.insert("cue track", TrackLayout::same("Small"));

    m
});

/// Get the layout for a group by name or path
///
/// Supports various lookup formats:
/// - Top-level: "Drums", "guitars", "VOCALS"
/// - Space-separated: "Electric Guitar", "Lead Vocals"
/// - Underscore-separated: "`electric_guitar`", "`lead_vocals`"
///
/// Returns `None` if no specific layout is defined for the given group.
///
/// # Example
/// ```
/// use dynamic_template::layouts::layout_for_group;
///
/// let layout = layout_for_group("Guide");
/// assert!(layout.is_some());
/// ```
pub fn layout_for_group(group: &str) -> Option<TrackLayout> {
    let normalized = group.to_lowercase();
    LAYOUT_MAP.get(normalized.as_str()).copied()
}

/// Get the layout for a group by its classification path
///
/// The path should be a list of group names from root to leaf,
/// e.g., `["Guitars", "Electric"]` or `["Vocals", "Background"]`.
///
/// Tries to find the most specific layout first, falling back to parent layouts.
pub fn layout_for_path(path: &[&str]) -> Option<TrackLayout> {
    if path.is_empty() {
        return None;
    }

    // Try the full path first (joined with "/")
    let full_path = path.join("/").to_lowercase();
    if let Some(&layout) = LAYOUT_MAP.get(full_path.as_str()) {
        return Some(layout);
    }

    // Try the last element (leaf group)
    let leaf = path.last()?.to_lowercase();
    if let Some(&layout) = LAYOUT_MAP.get(leaf.as_str()) {
        return Some(layout);
    }

    // Fall back through parent groups
    for i in (0..path.len()).rev() {
        if let Some(slice) = path.get(..=i) {
            let parent_path = slice.join("/").to_lowercase();
            if let Some(&layout) = LAYOUT_MAP.get(parent_path.as_str()) {
                return Some(layout);
            }
        }

        // Also try just the group name at this level
        if let Some(group) = path.get(i) {
            let group_name = group.to_lowercase();
            if let Some(&layout) = LAYOUT_MAP.get(group_name.as_str()) {
                return Some(layout);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_for_group() {
        assert!(layout_for_group("Guide").is_some());
        assert!(layout_for_group("guide").is_some());
        assert!(layout_for_group("Drums").is_some());
    }

    #[test]
    fn test_layout_for_path() {
        assert!(layout_for_path(&["Guide"]).is_some());
        assert!(layout_for_path(&["Drums"]).is_some());
        assert_eq!(layout_for_path(&[]), None);
    }
}
