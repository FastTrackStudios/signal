//! Setlist domain — ordered performance lists of songs.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::metadata::Metadata;
use crate::song::SongId;

crate::typed_uuid_id!(
    /// Identifies a Setlist collection.
    SetlistId
);
crate::typed_uuid_id!(
    /// Identifies a specific Setlist entry variant.
    SetlistEntryId
);

/// A setlist entry variant pointing to a song.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct SetlistEntry {
    pub id: SetlistEntryId,
    pub name: String,
    pub song_id: SongId,
    pub metadata: Metadata,
}

impl SetlistEntry {
    pub fn new(
        id: impl Into<SetlistEntryId>,
        name: impl Into<String>,
        song_id: impl Into<SongId>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            song_id: song_id.into(),
            metadata: Metadata::new(),
        }
    }

    /// Clone this entry with a new ID and name.
    pub fn duplicate(
        &self,
        new_id: impl Into<SetlistEntryId>,
        new_name: impl Into<String>,
    ) -> Self {
        let mut dup = self.clone();
        dup.id = new_id.into();
        dup.name = new_name.into();
        dup
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// A setlist collection — ordered list of entries with a default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Setlist {
    pub id: SetlistId,
    pub name: String,
    pub default_entry_id: SetlistEntryId,
    pub entries: Vec<SetlistEntry>,
    pub metadata: Metadata,
}

impl Setlist {
    pub fn new(
        id: impl Into<SetlistId>,
        name: impl Into<String>,
        default_entry: SetlistEntry,
    ) -> Self {
        let default_entry_id = default_entry.id.clone();
        Self {
            id: id.into(),
            name: name.into(),
            default_entry_id,
            entries: vec![default_entry],
            metadata: Metadata::new(),
        }
    }

    pub fn add_entry(&mut self, entry: SetlistEntry) {
        self.entries.push(entry);
    }

    pub fn entry(&self, id: &SetlistEntryId) -> Option<&SetlistEntry> {
        self.entries.iter().find(|e| &e.id == id)
    }

    pub fn entry_mut(&mut self, id: &SetlistEntryId) -> Option<&mut SetlistEntry> {
        self.entries.iter_mut().find(|e| &e.id == id)
    }

    pub fn remove_entry(&mut self, id: &SetlistEntryId) -> Option<SetlistEntry> {
        let pos = self.entries.iter().position(|e| &e.id == id)?;
        Some(self.entries.remove(pos))
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}

impl crate::traits::Variant for SetlistEntry {
    type Id = SetlistEntryId;
    type BaseRef = SongId;
    type Override = ();
    fn id(&self) -> &SetlistEntryId {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }
    fn base_ref(&self) -> Option<&Self::BaseRef> {
        Some(&self.song_id)
    }
}

impl crate::traits::DefaultVariant for SetlistEntry {
    fn default_named(name: impl Into<String>) -> Self {
        Self::new(SetlistEntryId::new(), name, SongId::new())
    }
}

impl crate::traits::Collection for Setlist {
    type Variant = SetlistEntry;
    fn variants(&self) -> &[SetlistEntry] {
        &self.entries
    }
    fn variants_mut(&mut self) -> &mut Vec<SetlistEntry> {
        &mut self.entries
    }
    fn default_variant_id(&self) -> &SetlistEntryId {
        &self.default_entry_id
    }
    fn set_default_variant_id(&mut self, id: SetlistEntryId) {
        self.default_entry_id = id;
    }
}

impl crate::traits::HasMetadata for SetlistEntry {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }
}

impl crate::traits::HasMetadata for Setlist {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }
}
