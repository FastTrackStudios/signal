//! Song operations — CRUD for songs and section variants.
//!
//! Provides [`SongOps`], a controller handle for managing songs,
//! their section variants, and section-to-patch/rig bindings.

use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::{
    metadata::Metadata,
    profile::ProfileId,
    song::{Section, SectionId, SectionSource, Song, SongId},
};

/// Handle for song operations.
pub struct SongOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> SongOps<S> {
    pub async fn list(&self) -> Result<Vec<Song>, OpsError> {
        self.0
            .service
            .list_songs()
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load(&self, id: impl Into<SongId>) -> Result<Option<Song>, OpsError> {
        self.0
            .service
            .load_song(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn create(
        &self,
        name: impl Into<String>,
        default_section_name: impl Into<String>,
        source: SectionSource,
    ) -> Result<Song, OpsError> {
        let song = Song::new(
            SongId::new(),
            name,
            Section {
                id: SectionId::new(),
                name: default_section_name.into(),
                source,
                overrides: Vec::new(),
                metadata: Metadata::new(),
            },
        );
        self.save(song.clone()).await?;
        Ok(song)
    }

    /// Create a song from a profile, generating one section per patch.
    ///
    /// Each section is named after its source patch and linked via
    /// `SectionSource::Patch`. The profile's ID is stored in the song's
    /// `metadata.base_profile_id`.
    pub async fn create_from_profile(
        &self,
        name: impl Into<String>,
        profile_id: impl Into<ProfileId>,
    ) -> Result<Song, OpsError> {
        let profile_id = profile_id.into();
        let profile = self
            .0
            .service
            .load_profile(profile_id.clone())
            .await
            .map_err(OpsError::Storage)?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "profile",
                id: profile_id.to_string(),
            })?;

        let song = Song::from_profile(SongId::new(), name, &profile);
        self.save(song.clone()).await?;
        Ok(song)
    }

    /// Change a song's base profile, remapping non-overridden sections.
    ///
    /// Sections still pointing at the old profile's patches are remapped by
    /// slot position to the new profile. Manually relinked sections are left
    /// untouched. Saves the song after remapping.
    pub async fn change_base_profile(
        &self,
        song_id: impl Into<SongId>,
        new_profile_id: impl Into<ProfileId>,
    ) -> Result<Song, OpsError> {
        let song_id = song_id.into();
        let new_profile_id = new_profile_id.into();

        let mut song = self
            .load(song_id)
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "song",
                id: "unknown".to_string(),
            })?;

        let new_profile = self
            .0
            .service
            .load_profile(new_profile_id.clone())
            .await
            .map_err(OpsError::Storage)?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "profile",
                id: new_profile_id.to_string(),
            })?;

        // Load the old base profile (if any) for slot remapping
        if let Some(old_id) = &song.metadata.base_profile_id {
            if let Some(old_profile) = self
                .0
                .service
                .load_profile(old_id.as_str().into())
                .await
                .map_err(OpsError::Storage)?
            {
                song.change_base_profile(&old_profile, &new_profile);
            } else {
                // Old profile was deleted — treat all sections as manually set,
                // just update the base_profile_id
                song.metadata.base_profile_id = Some(new_profile_id.to_string());
            }
        } else {
            // No previous base profile — just set it, don't remap anything
            song.metadata.base_profile_id = Some(new_profile_id.to_string());
        }

        self.save(song.clone()).await?;
        Ok(song)
    }

    pub async fn save(&self, song: Song) -> Result<Song, OpsError> {
        self.0
            .service
            .save_song(song.clone())
            .await
            .map_err(OpsError::Storage)?;
        Ok(song)
    }

    pub async fn delete(&self, id: impl Into<SongId>) -> Result<(), OpsError> {
        self.0
            .service
            .delete_song(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load_section(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
    ) -> Result<Option<Section>, OpsError> {
        self.0
            .service
            .load_song_variant(song_id.into(), section_id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn save_section(
        &self,
        song_id: impl Into<SongId>,
        section: Section,
    ) -> Result<(), OpsError> {
        let song_id = song_id.into();
        if let Some(mut song) = self.load(song_id).await? {
            if let Some(pos) = song.sections.iter().position(|s| s.id == section.id) {
                song.sections[pos] = section;
            } else {
                song.sections.push(section);
            }
            self.save(song).await?;
        }
        Ok(())
    }

    pub async fn set_section_source(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
        source: SectionSource,
    ) -> Result<(), OpsError> {
        let song_id = song_id.into();
        let section_id = section_id.into();
        if let Some(mut song) = self.load(song_id).await? {
            if let Some(section) = song.sections.iter_mut().find(|s| s.id == section_id) {
                section.source = source;
            }
            self.save(song).await?;
        }
        Ok(())
    }

    pub async fn reorder_sections(
        &self,
        song_id: impl Into<SongId>,
        ordered_section_ids: &[SectionId],
    ) -> Result<(), OpsError> {
        let song_id = song_id.into();
        if let Some(mut song) = self.load(song_id.clone()).await? {
            super::reorder_by_id(&mut song.sections, ordered_section_ids, |s| &s.id);
            self.save(song).await?;
        }
        Ok(())
    }

    pub async fn by_tag(&self, tag: &str) -> Result<Vec<Song>, OpsError> {
        let all = self.list().await?;
        Ok(all
            .into_iter()
            .filter(|s| s.metadata.tags.contains(tag))
            .collect())
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Song>, OpsError> {
        Ok(self.list().await?.into_iter().find(|s| s.name == name))
    }

    pub async fn rename(
        &self,
        id: impl Into<SongId>,
        new_name: impl Into<String>,
    ) -> Result<(), OpsError> {
        if let Some(mut song) = self.load(id).await? {
            song.name = new_name.into();
            self.save(song).await?;
        }
        Ok(())
    }

    /// Load a song, apply a closure to one of its sections, and save.
    pub async fn update_section(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
        f: impl FnOnce(&mut Section),
    ) -> Result<(), OpsError> {
        let song_id = song_id.into();
        let section_id = section_id.into();
        if let Some(mut song) = self.load(song_id).await? {
            if let Some(v) = song.sections.iter_mut().find(|s| s.id == section_id) {
                f(v);
            }
            self.save(song).await?;
        }
        Ok(())
    }

    /// Add a section to a song. Returns the updated song, or `None` if the song doesn't exist.
    pub async fn add_section(
        &self,
        song_id: impl Into<SongId>,
        section: Section,
    ) -> Result<Option<Song>, OpsError> {
        let song_id = song_id.into();
        if let Some(mut song) = self.load(song_id).await? {
            song.add_section(section);
            Ok(Some(self.save(song).await?))
        } else {
            Ok(None)
        }
    }

    /// Remove a section from a song. Returns the removed section, or `None` if not found.
    pub async fn remove_section(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
    ) -> Result<Option<Section>, OpsError> {
        let song_id = song_id.into();
        let section_id = section_id.into();
        if let Some(mut song) = self.load(song_id).await? {
            let removed = song.remove_section(&section_id);
            if removed.is_some() {
                self.save(song).await?;
            }
            Ok(removed)
        } else {
            Ok(None)
        }
    }

    /// Duplicate a section within a song. Returns the new section, or `None` if not found.
    pub async fn duplicate_section(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
        new_name: impl Into<String>,
    ) -> Result<Option<Section>, OpsError> {
        let song_id = song_id.into();
        let section_id = section_id.into();
        if let Some(mut song) = self.load(song_id).await? {
            if let Some(original) = song.section(&section_id) {
                let dup = original.duplicate(SectionId::new(), new_name);
                let dup_clone = dup.clone();
                song.add_section(dup);
                self.save(song).await?;
                Ok(Some(dup_clone))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Check if a song exists.
    pub async fn exists(&self, id: impl Into<SongId>) -> Result<bool, OpsError> {
        Ok(self.load(id).await?.is_some())
    }

    /// Count all songs.
    pub async fn count(&self) -> Result<usize, OpsError> {
        Ok(self.list().await?.len())
    }

    // region: --- try_* variants

    /// Add a section, returning an error if the song doesn't exist.
    pub async fn try_add_section(
        &self,
        song_id: impl Into<SongId>,
        section: Section,
    ) -> Result<Song, OpsError> {
        let song_id = song_id.into();
        let mut song = self
            .load(song_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "song",
                id: song_id.to_string(),
            })?;
        song.add_section(section);
        Ok(self.save(song).await?)
    }

    /// Remove a section, returning an error if the song or section doesn't exist.
    pub async fn try_remove_section(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
    ) -> Result<Section, OpsError> {
        let song_id = song_id.into();
        let section_id = section_id.into();
        let mut song = self
            .load(song_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "song",
                id: song_id.to_string(),
            })?;
        let removed =
            song.remove_section(&section_id)
                .ok_or_else(|| OpsError::VariantNotFound {
                    entity_type: "section",
                    parent_id: song_id.to_string(),
                    variant_id: section_id.to_string(),
                })?;
        self.save(song).await?;
        Ok(removed)
    }

    /// Duplicate a section, returning an error if the song or section doesn't exist.
    pub async fn try_duplicate_section(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
        new_name: impl Into<String>,
    ) -> Result<Section, OpsError> {
        let song_id = song_id.into();
        let section_id = section_id.into();
        let mut song = self
            .load(song_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "song",
                id: song_id.to_string(),
            })?;
        let original = song
            .section(&section_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "section",
                parent_id: song_id.to_string(),
                variant_id: section_id.to_string(),
            })?;
        let dup = original.duplicate(SectionId::new(), new_name);
        let dup_clone = dup.clone();
        song.add_section(dup);
        self.save(song).await?;
        Ok(dup_clone)
    }

    /// Save a section within a song, returning an error if the song doesn't exist.
    pub async fn try_save_section(
        &self,
        song_id: impl Into<SongId>,
        section: Section,
    ) -> Result<(), OpsError> {
        let song_id = song_id.into();
        let mut song = self
            .load(song_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "song",
                id: song_id.to_string(),
            })?;
        if let Some(pos) = song.sections.iter().position(|s| s.id == section.id) {
            song.sections[pos] = section;
        } else {
            song.sections.push(section);
        }
        self.save(song).await?;
        Ok(())
    }

    /// Update a section via closure, returning an error if the song or section doesn't exist.
    pub async fn try_update_section(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
        f: impl FnOnce(&mut Section),
    ) -> Result<(), OpsError> {
        let song_id = song_id.into();
        let section_id = section_id.into();
        let mut song = self
            .load(song_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "song",
                id: song_id.to_string(),
            })?;
        let section = song
            .sections
            .iter_mut()
            .find(|s| s.id == section_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "section",
                parent_id: song_id.to_string(),
                variant_id: section_id.to_string(),
            })?;
        f(section);
        self.save(song).await?;
        Ok(())
    }

    /// Set a section's source, returning an error if the song or section doesn't exist.
    pub async fn try_set_section_source(
        &self,
        song_id: impl Into<SongId>,
        section_id: impl Into<SectionId>,
        source: SectionSource,
    ) -> Result<(), OpsError> {
        let song_id = song_id.into();
        let section_id = section_id.into();
        let mut song = self
            .load(song_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "song",
                id: song_id.to_string(),
            })?;
        let section = song
            .sections
            .iter_mut()
            .find(|s| s.id == section_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "section",
                parent_id: song_id.to_string(),
                variant_id: section_id.to_string(),
            })?;
        section.source = source;
        self.save(song).await?;
        Ok(())
    }

    // endregion: --- try_* variants
}
