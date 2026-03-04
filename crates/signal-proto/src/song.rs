//! Song domain — performance songs with section variants.
//!
//! A [`Song`] is a collection of [`Section`] variants. Each Section
//! references either a Patch or a Rig variant, with optional overrides.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::metadata::Metadata;
use crate::override_policy::{validate_overrides, FreePolicy, OverridePolicyError};
use crate::overrides::Override;
use crate::profile::PatchId;
use crate::rig::{RigId, RigSceneId};

// ─── IDs ────────────────────────────────────────────────────────

crate::typed_uuid_id!(
    /// Identifies a Song collection.
    SongId
);
crate::typed_uuid_id!(
    /// Identifies a specific Section variant within a Song.
    SectionId
);

// ─── Section source ─────────────────────────────────────────────

/// What a song section references — either a Patch or a direct Rig variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum SectionSource {
    /// Reference a Patch from a Profile.
    Patch { patch_id: PatchId },
    /// Reference a Rig scene directly.
    RigScene { rig_id: RigId, scene_id: RigSceneId },
}

// ─── Section ────────────────────────────────────────────────────

/// A Section variant — one part of a song's performance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Section {
    pub id: SectionId,
    pub name: String,
    pub source: SectionSource,
    pub overrides: Vec<Override>,
    pub metadata: Metadata,
}

impl Section {
    pub fn from_patch(
        id: impl Into<SectionId>,
        name: impl Into<String>,
        patch_id: impl Into<PatchId>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source: SectionSource::Patch {
                patch_id: patch_id.into(),
            },
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    pub fn from_rig_scene(
        id: impl Into<SectionId>,
        name: impl Into<String>,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source: SectionSource::RigScene {
                rig_id: rig_id.into(),
                scene_id: scene_id.into(),
            },
            overrides: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    #[must_use]
    pub fn with_override(mut self, ov: Override) -> Self {
        self.overrides.push(ov);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn validate_overrides(&self) -> Result<(), OverridePolicyError> {
        validate_overrides::<FreePolicy>(&self.overrides)
    }

    /// Clone this section with a new ID and name.
    pub fn duplicate(&self, new_id: impl Into<SectionId>, new_name: impl Into<String>) -> Self {
        let mut dup = self.clone();
        dup.id = new_id.into();
        dup.name = new_name.into();
        dup
    }
}

// ─── Song ───────────────────────────────────────────────────────

/// A Song collection — performance structure with named sections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Song {
    pub id: SongId,
    pub name: String,
    pub artist: Option<String>,
    pub default_section_id: SectionId,
    pub sections: Vec<Section>,
    pub metadata: Metadata,
}

impl Song {
    pub fn new(id: impl Into<SongId>, name: impl Into<String>, default_section: Section) -> Self {
        let default_section_id = default_section.id.clone();
        Self {
            id: id.into(),
            name: name.into(),
            artist: None,
            default_section_id,
            sections: vec![default_section],
            metadata: Metadata::new(),
        }
    }

    pub fn add_section(&mut self, section: Section) {
        self.sections.push(section);
    }

    /// Semantic alias for `variants()` — returns all sections in this song.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    pub fn default_section(&self) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.id == self.default_section_id)
    }

    pub fn section(&self, id: &SectionId) -> Option<&Section> {
        self.sections.iter().find(|s| &s.id == id)
    }

    pub fn section_mut(&mut self, id: &SectionId) -> Option<&mut Section> {
        self.sections.iter_mut().find(|s| &s.id == id)
    }

    pub fn remove_section(&mut self, id: &SectionId) -> Option<Section> {
        let pos = self.sections.iter().position(|s| &s.id == id)?;
        Some(self.sections.remove(pos))
    }

    /// Create a Song from a Profile, generating one Section per Patch.
    ///
    /// Each section is named after its source patch and linked via
    /// `SectionSource::Patch { patch_id }`. The first patch becomes the
    /// default section. The profile's ID is stored in `metadata.base_profile_id`.
    pub fn from_profile(
        id: impl Into<SongId>,
        name: impl Into<String>,
        profile: &crate::profile::Profile,
    ) -> Self {
        let sections: Vec<Section> = profile
            .patches
            .iter()
            .map(|patch| Section::from_patch(SectionId::new(), &patch.name, patch.id.clone()))
            .collect();

        let default_section_id = sections
            .first()
            .map(|s| s.id.clone())
            .unwrap_or_else(SectionId::new);

        let metadata = Metadata::new().with_base_profile_id(profile.id.to_string());

        Self {
            id: id.into(),
            name: name.into(),
            artist: None,
            default_section_id,
            sections,
            metadata,
        }
    }

    /// Change the base profile, remapping sections that still follow the old profile.
    ///
    /// For each section whose `patch_id` belongs to the `old_profile`, finds its
    /// slot index and remaps it to the same slot in `new_profile`. Sections that
    /// were manually relinked (their `patch_id` is NOT in the old profile) are
    /// left untouched. Section names are updated to match the new patch name.
    ///
    /// If the new profile has more patches than the old one, extra sections are
    /// appended. If fewer, orphaned sections keep their old reference.
    pub fn change_base_profile(
        &mut self,
        old_profile: &crate::profile::Profile,
        new_profile: &crate::profile::Profile,
    ) {
        // Build old patch_id → slot index lookup
        let old_slots: std::collections::HashMap<String, usize> = old_profile
            .patches
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.to_string(), i))
            .collect();

        // Remap existing sections
        for section in &mut self.sections {
            if let SectionSource::Patch { patch_id } = &section.source {
                let pid_str = patch_id.to_string();
                if let Some(&slot) = old_slots.get(&pid_str) {
                    // This section followed the old profile at this slot
                    if let Some(new_patch) = new_profile.patches.get(slot) {
                        section.source = SectionSource::Patch {
                            patch_id: new_patch.id.clone(),
                        };
                        section.name = new_patch.name.clone();
                    }
                    // If new profile doesn't have this slot, leave section as-is
                }
                // If patch_id wasn't in old profile, it was manually relinked — skip
            }
            // RigScene sections are always manual — skip
        }

        // If new profile has more patches, append new sections for extra slots
        let existing_slot_count = old_profile.patches.len();
        for patch in new_profile.patches.iter().skip(existing_slot_count) {
            self.sections.push(Section::from_patch(
                SectionId::new(),
                &patch.name,
                patch.id.clone(),
            ));
        }

        self.metadata.base_profile_id = Some(new_profile.id.to_string());
    }

    #[must_use]
    pub fn with_artist(mut self, artist: impl Into<String>) -> Self {
        self.artist = Some(artist.into());
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}

// ─── Trait impls ────────────────────────────────────────────────

impl crate::traits::Variant for Section {
    type Id = SectionId;
    type BaseRef = SectionSource;
    type Override = Override;
    fn id(&self) -> &SectionId {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }
    fn base_ref(&self) -> Option<&Self::BaseRef> {
        Some(&self.source)
    }
    fn overrides(&self) -> Option<&[Self::Override]> {
        Some(&self.overrides)
    }
    fn overrides_mut(&mut self) -> Option<&mut Vec<Self::Override>> {
        Some(&mut self.overrides)
    }
}

impl crate::traits::DefaultVariant for Section {
    fn default_named(name: impl Into<String>) -> Self {
        Self::from_patch(SectionId::new(), name, PatchId::new())
    }
}

impl crate::traits::Collection for Song {
    type Variant = Section;

    fn variants(&self) -> &[Section] {
        &self.sections
    }
    fn variants_mut(&mut self) -> &mut Vec<Section> {
        &mut self.sections
    }
    fn default_variant_id(&self) -> &SectionId {
        &self.default_section_id
    }
    fn set_default_variant_id(&mut self, id: SectionId) {
        self.default_section_id = id;
    }
}

impl crate::traits::HasMetadata for Section {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }
}

impl crate::traits::HasMetadata for Song {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_song_with_patch_sections() {
        let verse = Section::from_patch(SectionId::new(), "Verse", PatchId::new());
        let chorus = Section::from_patch(SectionId::new(), "Chorus", PatchId::new());

        let mut song = Song::new(SongId::new(), "Amazing Grace", verse).with_artist("Traditional");
        song.add_section(chorus);

        assert_eq!(song.name, "Amazing Grace");
        assert_eq!(song.artist.as_deref(), Some("Traditional"));
        assert_eq!(song.sections.len(), 2);
        assert_eq!(song.default_section().unwrap().name, "Verse");
    }

    #[test]
    fn test_section_from_rig_scene() {
        let rig_id = RigId::new();
        let scene_id = RigSceneId::new();
        let section =
            Section::from_rig_scene(SectionId::new(), "Intro", rig_id.clone(), scene_id.clone());
        match &section.source {
            SectionSource::RigScene {
                rig_id: r,
                scene_id: s,
            } => {
                assert_eq!(r, &rig_id);
                assert_eq!(s, &scene_id);
            }
            _ => panic!("expected RigScene source"),
        }
    }

    #[test]
    fn test_song_from_profile() {
        use crate::profile::{Patch, PatchId, Profile, ProfileId};

        let rig_id = RigId::new();
        let scene_id = RigSceneId::new();

        let clean =
            Patch::from_rig_scene(PatchId::new(), "Clean", rig_id.clone(), scene_id.clone());
        let crunch =
            Patch::from_rig_scene(PatchId::new(), "Crunch", rig_id.clone(), scene_id.clone());
        let lead = Patch::from_rig_scene(PatchId::new(), "Lead", rig_id.clone(), scene_id.clone());

        let clean_pid = clean.id.clone();
        let crunch_pid = crunch.id.clone();
        let lead_pid = lead.id.clone();

        let mut profile = Profile::new(ProfileId::new(), "Rock Rig", clean);
        profile.add_patch(crunch);
        profile.add_patch(lead);

        let song = Song::from_profile(SongId::new(), "Girl Goodbye", &profile);

        // Correct number of sections
        assert_eq!(song.sections.len(), 3);

        // Each section is Patch-sourced with correct patch_id
        let expected_ids = [&clean_pid, &crunch_pid, &lead_pid];
        for (i, section) in song.sections.iter().enumerate() {
            match &section.source {
                SectionSource::Patch { patch_id } => {
                    assert_eq!(patch_id, expected_ids[i]);
                }
                _ => panic!("expected Patch source for section {}", i),
            }
        }

        // Section names match patch names
        assert_eq!(song.sections[0].name, "Clean");
        assert_eq!(song.sections[1].name, "Crunch");
        assert_eq!(song.sections[2].name, "Lead");

        // Default section is the first one
        assert_eq!(song.default_section_id, song.sections[0].id);

        // base_profile_id is set
        assert_eq!(
            song.metadata.base_profile_id.as_deref(),
            Some(profile.id.to_string().as_str())
        );
    }

    #[test]
    fn test_change_base_profile_remaps_slots() {
        use crate::profile::{Patch, PatchId, Profile, ProfileId};

        let rig_id = RigId::new();
        let scene_id = RigSceneId::new();

        // Old profile: Clean, Crunch, Lead
        let old_clean =
            Patch::from_rig_scene(PatchId::new(), "Clean", rig_id.clone(), scene_id.clone());
        let old_crunch =
            Patch::from_rig_scene(PatchId::new(), "Crunch", rig_id.clone(), scene_id.clone());
        let old_lead =
            Patch::from_rig_scene(PatchId::new(), "Lead", rig_id.clone(), scene_id.clone());
        let mut old_profile = Profile::new(ProfileId::new(), "Rock", old_clean);
        old_profile.add_patch(old_crunch);
        old_profile.add_patch(old_lead);

        // New profile: Shimmer, Funk, Solo, Ambient (4 patches — more than old)
        let new_shimmer =
            Patch::from_rig_scene(PatchId::new(), "Shimmer", rig_id.clone(), scene_id.clone());
        let new_funk =
            Patch::from_rig_scene(PatchId::new(), "Funk", rig_id.clone(), scene_id.clone());
        let new_solo =
            Patch::from_rig_scene(PatchId::new(), "Solo", rig_id.clone(), scene_id.clone());
        let new_ambient =
            Patch::from_rig_scene(PatchId::new(), "Ambient", rig_id.clone(), scene_id.clone());
        let new_shimmer_pid = new_shimmer.id.clone();
        let new_funk_pid = new_funk.id.clone();
        let new_solo_pid = new_solo.id.clone();
        let new_ambient_pid = new_ambient.id.clone();
        let mut new_profile = Profile::new(ProfileId::new(), "All-Around", new_shimmer);
        new_profile.add_patch(new_funk);
        new_profile.add_patch(new_solo);
        new_profile.add_patch(new_ambient);

        // Create song from old profile
        let mut song = Song::from_profile(SongId::new(), "Test Song", &old_profile);
        assert_eq!(song.sections.len(), 3);

        // Manually relink section 1 (Crunch) to a foreign patch
        let foreign_patch_id = PatchId::new();
        song.sections[1].source = SectionSource::Patch {
            patch_id: foreign_patch_id.clone(),
        };
        song.sections[1].name = "My Custom Patch".to_string();

        // Change base profile
        song.change_base_profile(&old_profile, &new_profile);

        // Should now have 4 sections (3 original + 1 new from extra slot)
        assert_eq!(song.sections.len(), 4);

        // Slot 0: was Clean → now Shimmer (remapped)
        assert_eq!(song.sections[0].name, "Shimmer");
        match &song.sections[0].source {
            SectionSource::Patch { patch_id } => assert_eq!(patch_id, &new_shimmer_pid),
            _ => panic!("expected Patch source"),
        }

        // Slot 1: was manually relinked → untouched
        assert_eq!(song.sections[1].name, "My Custom Patch");
        match &song.sections[1].source {
            SectionSource::Patch { patch_id } => assert_eq!(patch_id, &foreign_patch_id),
            _ => panic!("expected Patch source"),
        }

        // Slot 2: was Lead → now Solo (remapped)
        assert_eq!(song.sections[2].name, "Solo");
        match &song.sections[2].source {
            SectionSource::Patch { patch_id } => assert_eq!(patch_id, &new_solo_pid),
            _ => panic!("expected Patch source"),
        }

        // Slot 3: new section from extra patch (Ambient)
        assert_eq!(song.sections[3].name, "Ambient");
        match &song.sections[3].source {
            SectionSource::Patch { patch_id } => assert_eq!(patch_id, &new_ambient_pid),
            _ => panic!("expected Patch source"),
        }

        // base_profile_id updated
        assert_eq!(
            song.metadata.base_profile_id.as_deref(),
            Some(new_profile.id.to_string().as_str())
        );
    }
}
