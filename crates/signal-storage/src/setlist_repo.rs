//! Setlist repository — data access for Setlist collections and SetlistEntry variants.

use sea_orm::*;
use signal_proto::metadata::Metadata;
use signal_proto::setlist::{Setlist, SetlistEntry, SetlistEntryId, SetlistId};
use signal_proto::song::SongId;

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

#[async_trait::async_trait]
pub trait SetlistRepo: Send + Sync {
    async fn list_setlists(&self) -> StorageResult<Vec<Setlist>>;
    async fn load_setlist(&self, id: &SetlistId) -> StorageResult<Option<Setlist>>;
    async fn save_setlist(&self, setlist: &Setlist) -> StorageResult<()>;
    async fn delete_setlist(&self, id: &SetlistId) -> StorageResult<()>;
    async fn load_entry(
        &self,
        setlist_id: &SetlistId,
        entry_id: &SetlistEntryId,
    ) -> StorageResult<Option<SetlistEntry>>;
}

#[derive(Clone)]
pub struct SetlistRepoLive {
    db: DatabaseConnection,
}

impl SetlistRepoLive {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut setlists = schema.create_table_from_entity(entity::setlist::Entity);
        setlists.if_not_exists();
        self.db.execute(backend.build(&setlists)).await?;

        let mut entries = schema.create_table_from_entity(entity::setlist_entry::Entity);
        entries.if_not_exists();
        self.db.execute(backend.build(&entries)).await?;

        Ok(())
    }

    fn entry_state_to_json(entry: &SetlistEntry) -> StorageResult<String> {
        let state = EntryState {
            song_id: entry.song_id.as_str(),
        };
        serde_json::to_string(&state)
            .map_err(|e| StorageError::Data(format!("failed to serialize setlist entry: {e}")))
    }

    fn entry_state_from_json(json: &str) -> StorageResult<EntryStateOwned> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse setlist entry json: {e}")))
    }

    fn metadata_to_json(metadata: &Metadata) -> StorageResult<String> {
        serde_json::to_string(metadata)
            .map_err(|e| StorageError::Data(format!("failed to serialize metadata: {e}")))
    }

    fn metadata_from_json(json: &str) -> StorageResult<Metadata> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse metadata json: {e}")))
    }

    fn entry_from_model(model: &entity::setlist_entry::Model) -> StorageResult<SetlistEntry> {
        let state = Self::entry_state_from_json(&model.state_json)?;
        let metadata = Self::metadata_from_json(&model.metadata_json)?;
        Ok(SetlistEntry {
            id: model.entry_id_branded(),
            name: model.name.clone(),
            song_id: SongId::from(state.song_id),
            metadata,
        })
    }

    async fn assemble_setlist(&self, model: &entity::setlist::Model) -> StorageResult<Setlist> {
        let entry_models = entity::setlist_entry::Entity::find()
            .filter(entity::setlist_entry::Column::SetlistId.eq(model.id.clone()))
            .order_by_asc(entity::setlist_entry::Column::Position)
            .all(&self.db)
            .await?;

        let mut entries = Vec::with_capacity(entry_models.len());
        for em in &entry_models {
            entries.push(Self::entry_from_model(em)?);
        }

        let metadata = Self::metadata_from_json(&model.metadata_json)?;

        Ok(Setlist {
            id: model.setlist_id_branded(),
            name: model.name.clone(),
            default_entry_id: model.default_entry_id_branded(),
            entries,
            metadata,
        })
    }
}

#[derive(serde::Serialize)]
struct EntryState<'a> {
    song_id: &'a str,
}

#[derive(serde::Deserialize)]
struct EntryStateOwned {
    song_id: String,
}

#[async_trait::async_trait]
impl SetlistRepo for SetlistRepoLive {
    async fn list_setlists(&self) -> StorageResult<Vec<Setlist>> {
        let models = entity::setlist::Entity::find()
            .order_by_asc(entity::setlist::Column::Id)
            .all(&self.db)
            .await?;

        let mut out = Vec::with_capacity(models.len());
        for model in &models {
            out.push(self.assemble_setlist(model).await?);
        }
        Ok(out)
    }

    async fn load_setlist(&self, id: &SetlistId) -> StorageResult<Option<Setlist>> {
        let model = entity::setlist::Entity::find_by_id(id.as_str().to_string())
            .one(&self.db)
            .await?;
        match model {
            Some(ref m) => Ok(Some(self.assemble_setlist(m).await?)),
            None => Ok(None),
        }
    }

    async fn save_setlist(&self, setlist: &Setlist) -> StorageResult<()> {
        entity::setlist::Entity::delete_by_id(setlist.id.to_string())
            .exec(&self.db)
            .await
            .ok();

        entity::setlist::Entity::insert(entity::setlist::ActiveModel {
            id: Set(setlist.id.to_string()),
            name: Set(setlist.name.clone()),
            default_entry_id: Set(setlist.default_entry_id.to_string()),
            metadata_json: Set(Self::metadata_to_json(&setlist.metadata)?),
        })
        .exec(&self.db)
        .await?;

        entity::setlist_entry::Entity::delete_many()
            .filter(entity::setlist_entry::Column::SetlistId.eq(setlist.id.to_string()))
            .exec(&self.db)
            .await?;

        for (position, entry) in setlist.entries.iter().enumerate() {
            entity::setlist_entry::Entity::insert(entity::setlist_entry::ActiveModel {
                id: Set(entry.id.to_string()),
                setlist_id: Set(setlist.id.to_string()),
                position: Set(position as i32),
                name: Set(entry.name.clone()),
                state_json: Set(Self::entry_state_to_json(entry)?),
                metadata_json: Set(Self::metadata_to_json(&entry.metadata)?),
            })
            .exec(&self.db)
            .await?;
        }

        Ok(())
    }

    async fn delete_setlist(&self, id: &SetlistId) -> StorageResult<()> {
        entity::setlist::Entity::delete_by_id(id.as_str().to_string())
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn load_entry(
        &self,
        setlist_id: &SetlistId,
        entry_id: &SetlistEntryId,
    ) -> StorageResult<Option<SetlistEntry>> {
        let model = entity::setlist_entry::Entity::find_by_id(entry_id.as_str().to_string())
            .filter(entity::setlist_entry::Column::SetlistId.eq(setlist_id.to_string()))
            .one(&self.db)
            .await?;
        match model {
            Some(model) => Ok(Some(Self::entry_from_model(&model)?)),
            None => Ok(None),
        }
    }
}
