//! Shared metadata for collections and variants.
//!
//! [`Metadata`] provides tags, description, and notes that can be attached
//! to any domain entity (collections, variants, templates, etc.).

use facet::Facet;
use serde::{Deserialize, Serialize};

// ─── Tags ───────────────────────────────────────────────────────

/// A set of string tags for categorization and filtering.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
pub struct Tags(Vec<String>);

impl Tags {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn from_vec(tags: Vec<String>) -> Self {
        Self(tags)
    }

    pub fn add(&mut self, tag: impl Into<String>) {
        let tag = tag.into();
        if !self.0.contains(&tag) {
            self.0.push(tag);
        }
    }

    pub fn remove(&mut self, tag: &str) {
        self.0.retain(|t| t != tag);
    }

    pub fn contains(&self, tag: &str) -> bool {
        self.0.iter().any(|t| t == tag)
    }

    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

// ─── Metadata ───────────────────────────────────────────────────

/// Shared metadata for any domain entity.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct Metadata {
    pub tags: Tags,
    pub description: Option<String>,
    pub notes: Option<String>,
    /// Folder path for organizing this entity in a hierarchy.
    ///
    /// Uses forward-slash separators (e.g., `"Artists/Cory Wong"`, `"John Mayer"`).
    /// `None` means the entity lives at the root level of its collection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// The profile used as the base template when this song was created.
    ///
    /// Stores the profile ID as a string. `None` means the song was created
    /// without a base profile (or predates this field).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_profile_id: Option<String>,
}

impl Metadata {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    #[must_use]
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.add(tag);
        self
    }

    #[must_use]
    pub fn with_folder(mut self, folder: impl Into<String>) -> Self {
        self.folder = Some(folder.into());
        self
    }

    #[must_use]
    pub fn with_base_profile_id(mut self, id: impl Into<String>) -> Self {
        self.base_profile_id = Some(id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tags_add_remove() {
        let mut tags = Tags::new();
        tags.add("rock");
        tags.add("blues");
        tags.add("rock"); // duplicate — ignored
        assert_eq!(tags.len(), 2);
        assert!(tags.contains("rock"));

        tags.remove("rock");
        assert!(!tags.contains("rock"));
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn test_metadata_builder() {
        let meta = Metadata::new()
            .with_description("A cool preset")
            .with_notes("Tweak the gain for taste")
            .with_tag("guitar")
            .with_tag("high-gain");

        assert_eq!(meta.description.as_deref(), Some("A cool preset"));
        assert_eq!(meta.notes.as_deref(), Some("Tweak the gain for taste"));
        assert_eq!(meta.tags.len(), 2);
        assert_eq!(meta.folder, None);
    }

    #[test]
    fn test_metadata_with_folder() {
        let meta = Metadata::new()
            .with_folder("Artists/Cory Wong")
            .with_tag("Clean");

        assert_eq!(meta.folder.as_deref(), Some("Artists/Cory Wong"));
        assert!(meta.tags.contains("Clean"));
    }

    #[test]
    fn test_metadata_folder_backwards_compat() {
        // Existing JSON without folder field should deserialize with folder=None
        let json = r#"{"tags":["rock"],"description":"Old preset","notes":null}"#;
        let meta: Metadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.folder, None);
        assert!(meta.tags.contains("rock"));
    }

    #[test]
    fn test_metadata_folder_skip_serializing_none() {
        // folder=None should not appear in JSON output
        let meta = Metadata::new().with_tag("blues");
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("folder"), "folder=None should be omitted");
    }

    #[test]
    fn test_metadata_folder_round_trip() {
        let meta = Metadata::new()
            .with_folder("John Mayer")
            .with_tag("Clean")
            .with_description("A clean tone");
        let json = serde_json::to_string(&meta).unwrap();
        let roundtrip: Metadata = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.folder.as_deref(), Some("John Mayer"));
    }

    #[test]
    fn test_metadata_base_profile_backwards_compat() {
        let json = r#"{"tags":["rock"],"description":"Old song","notes":null}"#;
        let meta: Metadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.base_profile_id, None);
    }

    #[test]
    fn test_metadata_base_profile_skip_serializing_none() {
        let meta = Metadata::new().with_tag("blues");
        let json = serde_json::to_string(&meta).unwrap();
        assert!(
            !json.contains("base_profile_id"),
            "base_profile_id=None should be omitted"
        );
    }

    #[test]
    fn test_metadata_base_profile_round_trip() {
        let meta = Metadata::new()
            .with_base_profile_id("some-profile-uuid")
            .with_tag("worship");
        let json = serde_json::to_string(&meta).unwrap();
        let roundtrip: Metadata = serde_json::from_str(&json).unwrap();
        assert_eq!(
            roundtrip.base_profile_id.as_deref(),
            Some("some-profile-uuid")
        );
    }
}
