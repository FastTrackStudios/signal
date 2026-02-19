//! DAW snapshot repository — stores captured DAW parameter state and binary chunks.
//!
//! These snapshots are captured from the DAW's FX chain and stored for recall.
//! Each snapshot is keyed by an `owner_id` (rig ID, scene ID, etc.) so multiple
//! snapshots can be grouped under a parent entity.

use sea_orm::*;

use crate::entity;
use crate::{DatabaseConnection, StorageResult};

// region: --- Domain types (lightweight wrappers for DB rows)

/// A stored parameter snapshot.
#[derive(Debug, Clone)]
pub struct StoredParamSnapshot {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    /// JSON-serialized parameter map.
    pub params_json: String,
    pub created_at: String,
}

/// A stored binary state chunk.
#[derive(Debug, Clone)]
pub struct StoredChunkSnapshot {
    pub id: String,
    pub owner_id: String,
    pub fx_id: String,
    pub plugin_name: String,
    /// Base64-encoded binary data.
    pub chunk_data_b64: String,
    pub created_at: String,
}

// endregion: --- Domain types

// region: --- Trait

#[async_trait::async_trait]
pub trait DawSnapshotRepo: Send + Sync {
    // Parameter snapshots
    async fn list_param_snapshots(
        &self,
        owner_id: &str,
    ) -> StorageResult<Vec<StoredParamSnapshot>>;
    async fn save_param_snapshot(&self, snapshot: &StoredParamSnapshot) -> StorageResult<()>;
    async fn delete_param_snapshot(&self, id: &str) -> StorageResult<()>;
    async fn delete_param_snapshots_by_owner(&self, owner_id: &str) -> StorageResult<()>;

    // Chunk snapshots
    async fn list_chunk_snapshots(
        &self,
        owner_id: &str,
    ) -> StorageResult<Vec<StoredChunkSnapshot>>;
    async fn save_chunk_snapshot(&self, snapshot: &StoredChunkSnapshot) -> StorageResult<()>;
    async fn delete_chunk_snapshot(&self, id: &str) -> StorageResult<()>;
    async fn delete_chunk_snapshots_by_owner(&self, owner_id: &str) -> StorageResult<()>;
}

// endregion: --- Trait

// region: --- DawSnapshotRepoLive

#[derive(Clone)]
pub struct DawSnapshotRepoLive {
    db: DatabaseConnection,
}

impl DawSnapshotRepoLive {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut params = schema.create_table_from_entity(entity::daw_snapshot::Entity);
        params.if_not_exists();
        self.db.execute(backend.build(&params)).await?;

        let mut chunks =
            schema.create_table_from_entity(entity::daw_snapshot::chunk::Entity);
        chunks.if_not_exists();
        self.db.execute(backend.build(&chunks)).await?;

        Ok(())
    }
}

// endregion: --- DawSnapshotRepoLive

// region: --- Trait impl

#[async_trait::async_trait]
impl DawSnapshotRepo for DawSnapshotRepoLive {
    // --- Parameter snapshots ---

    async fn list_param_snapshots(
        &self,
        owner_id: &str,
    ) -> StorageResult<Vec<StoredParamSnapshot>> {
        let models = entity::daw_snapshot::Entity::find()
            .filter(entity::daw_snapshot::Column::OwnerId.eq(owner_id.to_string()))
            .order_by_asc(entity::daw_snapshot::Column::CreatedAt)
            .all(&self.db)
            .await?;

        Ok(models
            .into_iter()
            .map(|m| StoredParamSnapshot {
                id: m.id,
                owner_id: m.owner_id,
                name: m.name,
                params_json: m.params_json,
                created_at: m.created_at,
            })
            .collect())
    }

    async fn save_param_snapshot(&self, snapshot: &StoredParamSnapshot) -> StorageResult<()> {
        entity::daw_snapshot::Entity::delete_by_id(snapshot.id.clone())
            .exec(&self.db)
            .await
            .ok();

        entity::daw_snapshot::Entity::insert(entity::daw_snapshot::ActiveModel {
            id: Set(snapshot.id.clone()),
            owner_id: Set(snapshot.owner_id.clone()),
            name: Set(snapshot.name.clone()),
            params_json: Set(snapshot.params_json.clone()),
            created_at: Set(snapshot.created_at.clone()),
        })
        .exec(&self.db)
        .await?;

        Ok(())
    }

    async fn delete_param_snapshot(&self, id: &str) -> StorageResult<()> {
        entity::daw_snapshot::Entity::delete_by_id(id.to_string())
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn delete_param_snapshots_by_owner(&self, owner_id: &str) -> StorageResult<()> {
        entity::daw_snapshot::Entity::delete_many()
            .filter(entity::daw_snapshot::Column::OwnerId.eq(owner_id.to_string()))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    // --- Chunk snapshots ---

    async fn list_chunk_snapshots(
        &self,
        owner_id: &str,
    ) -> StorageResult<Vec<StoredChunkSnapshot>> {
        let models = entity::daw_snapshot::chunk::Entity::find()
            .filter(entity::daw_snapshot::chunk::Column::OwnerId.eq(owner_id.to_string()))
            .order_by_asc(entity::daw_snapshot::chunk::Column::CreatedAt)
            .all(&self.db)
            .await?;

        Ok(models
            .into_iter()
            .map(|m| StoredChunkSnapshot {
                id: m.id,
                owner_id: m.owner_id,
                fx_id: m.fx_id,
                plugin_name: m.plugin_name,
                chunk_data_b64: m.chunk_data_b64,
                created_at: m.created_at,
            })
            .collect())
    }

    async fn save_chunk_snapshot(&self, snapshot: &StoredChunkSnapshot) -> StorageResult<()> {
        entity::daw_snapshot::chunk::Entity::delete_by_id(snapshot.id.clone())
            .exec(&self.db)
            .await
            .ok();

        entity::daw_snapshot::chunk::Entity::insert(
            entity::daw_snapshot::chunk::ActiveModel {
                id: Set(snapshot.id.clone()),
                owner_id: Set(snapshot.owner_id.clone()),
                fx_id: Set(snapshot.fx_id.clone()),
                plugin_name: Set(snapshot.plugin_name.clone()),
                chunk_data_b64: Set(snapshot.chunk_data_b64.clone()),
                created_at: Set(snapshot.created_at.clone()),
            },
        )
        .exec(&self.db)
        .await?;

        Ok(())
    }

    async fn delete_chunk_snapshot(&self, id: &str) -> StorageResult<()> {
        entity::daw_snapshot::chunk::Entity::delete_by_id(id.to_string())
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn delete_chunk_snapshots_by_owner(&self, owner_id: &str) -> StorageResult<()> {
        entity::daw_snapshot::chunk::Entity::delete_many()
            .filter(entity::daw_snapshot::chunk::Column::OwnerId.eq(owner_id.to_string()))
            .exec(&self.db)
            .await?;
        Ok(())
    }
}

// endregion: --- Trait impl

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;

    async fn test_repo() -> StorageResult<DawSnapshotRepoLive> {
        let db = Database::connect("sqlite::memory:").await?;
        let repo = DawSnapshotRepoLive::new(db);
        repo.init_schema().await?;
        Ok(repo)
    }

    #[tokio::test]
    async fn param_snapshot_round_trip() {
        let repo = test_repo().await.unwrap();
        let snap = StoredParamSnapshot {
            id: "snap-1".into(),
            owner_id: "rig-1".into(),
            name: "Clean params".into(),
            params_json: r#"{"gain":0.5}"#.into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };

        repo.save_param_snapshot(&snap).await.unwrap();
        let loaded = repo.list_param_snapshots("rig-1").await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Clean params");
        assert_eq!(loaded[0].params_json, r#"{"gain":0.5}"#);
    }

    #[tokio::test]
    async fn chunk_snapshot_round_trip() {
        let repo = test_repo().await.unwrap();
        let snap = StoredChunkSnapshot {
            id: "chunk-1".into(),
            owner_id: "rig-1".into(),
            fx_id: "fx-guid-abc".into(),
            plugin_name: "Helix Native".into(),
            chunk_data_b64: "AAECBA==".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };

        repo.save_chunk_snapshot(&snap).await.unwrap();
        let loaded = repo.list_chunk_snapshots("rig-1").await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].plugin_name, "Helix Native");
    }

    #[tokio::test]
    async fn delete_by_owner_clears_all() {
        let repo = test_repo().await.unwrap();

        for i in 0..3 {
            repo.save_param_snapshot(&StoredParamSnapshot {
                id: format!("snap-{i}"),
                owner_id: "rig-1".into(),
                name: format!("Snap {i}"),
                params_json: "{}".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
            })
            .await
            .unwrap();
        }

        assert_eq!(repo.list_param_snapshots("rig-1").await.unwrap().len(), 3);
        repo.delete_param_snapshots_by_owner("rig-1")
            .await
            .unwrap();
        assert_eq!(repo.list_param_snapshots("rig-1").await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn different_owners_isolated() {
        let repo = test_repo().await.unwrap();

        repo.save_param_snapshot(&StoredParamSnapshot {
            id: "snap-a".into(),
            owner_id: "rig-1".into(),
            name: "A".into(),
            params_json: "{}".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        })
        .await
        .unwrap();

        repo.save_param_snapshot(&StoredParamSnapshot {
            id: "snap-b".into(),
            owner_id: "rig-2".into(),
            name: "B".into(),
            params_json: "{}".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        })
        .await
        .unwrap();

        assert_eq!(repo.list_param_snapshots("rig-1").await.unwrap().len(), 1);
        assert_eq!(repo.list_param_snapshots("rig-2").await.unwrap().len(), 1);
    }
}

// endregion: --- Tests
