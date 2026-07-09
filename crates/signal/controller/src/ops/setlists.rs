//! Setlist operations — CRUD for setlists and their song entries.
//!
//! Provides [`SetlistOps`], a controller handle for managing ordered
//! performance setlists and their constituent song entries.

use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::{
    setlist::{Setlist, SetlistEntry, SetlistEntryId, SetlistId},
    song::SongId,
};

/// Handle for setlist operations.
pub struct SetlistOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> SetlistOps<S> {
    pub async fn list(&self) -> Result<Vec<Setlist>, OpsError> {
        self.0
            .service
            .list_setlists()
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load(&self, id: impl Into<SetlistId>) -> Result<Option<Setlist>, OpsError> {
        self.0
            .service
            .load_setlist(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn create(
        &self,
        name: impl Into<String>,
        default_entry_name: impl Into<String>,
        song_id: impl Into<SongId>,
    ) -> Result<Setlist, OpsError> {
        let setlist = Setlist::new(
            SetlistId::new(),
            name,
            SetlistEntry::new(SetlistEntryId::new(), default_entry_name, song_id),
        );
        self.save(setlist.clone()).await?;
        Ok(setlist)
    }

    pub async fn save(&self, setlist: Setlist) -> Result<Setlist, OpsError> {
        self.0
            .service
            .save_setlist(setlist.clone())
            .await
            .map_err(OpsError::Storage)?;
        Ok(setlist)
    }

    pub async fn delete(&self, id: impl Into<SetlistId>) -> Result<(), OpsError> {
        self.0
            .service
            .delete_setlist(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry_id: impl Into<SetlistEntryId>,
    ) -> Result<Option<SetlistEntry>, OpsError> {
        self.0
            .service
            .load_setlist_entry(setlist_id.into(), entry_id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn save_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry: SetlistEntry,
    ) -> Result<(), OpsError> {
        let setlist_id = setlist_id.into();
        if let Some(mut setlist) = self.load(setlist_id).await? {
            if let Some(pos) = setlist.entries.iter().position(|e| e.id == entry.id) {
                setlist.entries[pos] = entry;
            } else {
                setlist.entries.push(entry);
            }
            self.save(setlist).await?;
        }
        Ok(())
    }

    pub async fn reorder_entries(
        &self,
        setlist_id: impl Into<SetlistId>,
        ordered_entry_ids: &[SetlistEntryId],
    ) -> Result<(), OpsError> {
        let setlist_id = setlist_id.into();
        if let Some(mut setlist) = self.load(setlist_id.clone()).await? {
            super::reorder_by_id(&mut setlist.entries, ordered_entry_ids, |e| &e.id);
            self.save(setlist).await?;
        }
        Ok(())
    }

    pub async fn by_tag(&self, tag: &str) -> Result<Vec<Setlist>, OpsError> {
        let all = self.list().await?;
        Ok(all
            .into_iter()
            .filter(|s| s.metadata.tags.contains(tag))
            .collect())
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Setlist>, OpsError> {
        Ok(self.list().await?.into_iter().find(|s| s.name == name))
    }

    pub async fn rename(
        &self,
        id: impl Into<SetlistId>,
        new_name: impl Into<String>,
    ) -> Result<(), OpsError> {
        if let Some(mut setlist) = self.load(id).await? {
            setlist.name = new_name.into();
            self.save(setlist).await?;
        }
        Ok(())
    }

    /// Load a setlist, apply a closure to one of its entries, and save.
    pub async fn update_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry_id: impl Into<SetlistEntryId>,
        f: impl FnOnce(&mut SetlistEntry),
    ) -> Result<(), OpsError> {
        let setlist_id = setlist_id.into();
        let entry_id = entry_id.into();
        if let Some(mut setlist) = self.load(setlist_id).await? {
            if let Some(v) = setlist.entries.iter_mut().find(|e| e.id == entry_id) {
                f(v);
            }
            self.save(setlist).await?;
        }
        Ok(())
    }

    /// Add an entry to a setlist. Returns the updated setlist, or `None` if the setlist doesn't exist.
    pub async fn add_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry: SetlistEntry,
    ) -> Result<Option<Setlist>, OpsError> {
        let setlist_id = setlist_id.into();
        if let Some(mut setlist) = self.load(setlist_id).await? {
            setlist.add_entry(entry);
            Ok(Some(self.save(setlist).await?))
        } else {
            Ok(None)
        }
    }

    /// Remove an entry from a setlist. Returns the removed entry, or `None` if not found.
    pub async fn remove_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry_id: impl Into<SetlistEntryId>,
    ) -> Result<Option<SetlistEntry>, OpsError> {
        let setlist_id = setlist_id.into();
        let entry_id = entry_id.into();
        if let Some(mut setlist) = self.load(setlist_id).await? {
            let removed = setlist.remove_entry(&entry_id);
            if removed.is_some() {
                self.save(setlist).await?;
            }
            Ok(removed)
        } else {
            Ok(None)
        }
    }

    /// Duplicate an entry within a setlist. Returns the new entry, or `None` if not found.
    pub async fn duplicate_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry_id: impl Into<SetlistEntryId>,
        new_name: impl Into<String>,
    ) -> Result<Option<SetlistEntry>, OpsError> {
        let setlist_id = setlist_id.into();
        let entry_id = entry_id.into();
        if let Some(mut setlist) = self.load(setlist_id).await? {
            if let Some(original) = setlist.entry(&entry_id) {
                let dup = original.duplicate(SetlistEntryId::new(), new_name);
                let dup_clone = dup.clone();
                setlist.add_entry(dup);
                self.save(setlist).await?;
                Ok(Some(dup_clone))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Check if a setlist exists.
    pub async fn exists(&self, id: impl Into<SetlistId>) -> Result<bool, OpsError> {
        Ok(self.load(id).await?.is_some())
    }

    /// Count all setlists.
    pub async fn count(&self) -> Result<usize, OpsError> {
        Ok(self.list().await?.len())
    }

    // region: --- try_* variants

    /// Add an entry, returning an error if the setlist doesn't exist.
    pub async fn try_add_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry: SetlistEntry,
    ) -> Result<Setlist, OpsError> {
        let setlist_id = setlist_id.into();
        let mut setlist =
            self.load(setlist_id.clone())
                .await?
                .ok_or_else(|| OpsError::NotFound {
                    entity_type: "setlist",
                    id: setlist_id.to_string(),
                })?;
        setlist.add_entry(entry);
        Ok(self.save(setlist).await?)
    }

    /// Remove an entry, returning an error if the setlist or entry doesn't exist.
    pub async fn try_remove_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry_id: impl Into<SetlistEntryId>,
    ) -> Result<SetlistEntry, OpsError> {
        let setlist_id = setlist_id.into();
        let entry_id = entry_id.into();
        let mut setlist =
            self.load(setlist_id.clone())
                .await?
                .ok_or_else(|| OpsError::NotFound {
                    entity_type: "setlist",
                    id: setlist_id.to_string(),
                })?;
        let removed = setlist
            .remove_entry(&entry_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "entry",
                parent_id: setlist_id.to_string(),
                variant_id: entry_id.to_string(),
            })?;
        self.save(setlist).await?;
        Ok(removed)
    }

    /// Duplicate an entry, returning an error if the setlist or entry doesn't exist.
    pub async fn try_duplicate_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry_id: impl Into<SetlistEntryId>,
        new_name: impl Into<String>,
    ) -> Result<SetlistEntry, OpsError> {
        let setlist_id = setlist_id.into();
        let entry_id = entry_id.into();
        let mut setlist =
            self.load(setlist_id.clone())
                .await?
                .ok_or_else(|| OpsError::NotFound {
                    entity_type: "setlist",
                    id: setlist_id.to_string(),
                })?;
        let original = setlist
            .entry(&entry_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "entry",
                parent_id: setlist_id.to_string(),
                variant_id: entry_id.to_string(),
            })?;
        let dup = original.duplicate(SetlistEntryId::new(), new_name);
        let dup_clone = dup.clone();
        setlist.add_entry(dup);
        self.save(setlist).await?;
        Ok(dup_clone)
    }

    /// Save an entry within a setlist, returning an error if the setlist doesn't exist.
    pub async fn try_save_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry: SetlistEntry,
    ) -> Result<(), OpsError> {
        let setlist_id = setlist_id.into();
        let mut setlist =
            self.load(setlist_id.clone())
                .await?
                .ok_or_else(|| OpsError::NotFound {
                    entity_type: "setlist",
                    id: setlist_id.to_string(),
                })?;
        if let Some(pos) = setlist.entries.iter().position(|e| e.id == entry.id) {
            setlist.entries[pos] = entry;
        } else {
            setlist.entries.push(entry);
        }
        self.save(setlist).await?;
        Ok(())
    }

    /// Update an entry via closure, returning an error if the setlist or entry doesn't exist.
    pub async fn try_update_entry(
        &self,
        setlist_id: impl Into<SetlistId>,
        entry_id: impl Into<SetlistEntryId>,
        f: impl FnOnce(&mut SetlistEntry),
    ) -> Result<(), OpsError> {
        let setlist_id = setlist_id.into();
        let entry_id = entry_id.into();
        let mut setlist =
            self.load(setlist_id.clone())
                .await?
                .ok_or_else(|| OpsError::NotFound {
                    entity_type: "setlist",
                    id: setlist_id.to_string(),
                })?;
        let entry = setlist
            .entries
            .iter_mut()
            .find(|e| e.id == entry_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "entry",
                parent_id: setlist_id.to_string(),
                variant_id: entry_id.to_string(),
            })?;
        f(entry);
        self.save(setlist).await?;
        Ok(())
    }

    // endregion: --- try_* variants
}
