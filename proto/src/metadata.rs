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
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    #[must_use]
    pub const fn from_vec(tags: Vec<String>) -> Self {
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

    #[must_use]
    pub fn contains(&self, tag: &str) -> bool {
        self.0.iter().any(|t| t == tag)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }
}

// ─── Metadata ───────────────────────────────────────────────────

/// Shared metadata for any domain entity.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
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
    #[must_use]
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
