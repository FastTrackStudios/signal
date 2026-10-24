//! Song repository — data access for Song collections and Scene variants.

use sea_orm::{
    ActiveEnum, ActiveModelBehavior, ActiveModelTrait, ColIdx, ColumnTrait, ConnectionTrait,
    EntityTrait, Iden, IdenStatic, Iterable, QueryFilter, QueryOrder, QueryTrait, Schema, Set,
    StatementBuilder,
};
use signal_proto::metadata::Metadata;
use signal_proto::overrides::Override;
use signal_proto::song::{Scene, SceneId, SceneSource, Song, SongId};

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait SongRepo: Send + Sync + 'static {
    async fn list_songs(&self) -> StorageResult<Vec<Song>>;
    async fn load_song(&self, id: &SongId) -> StorageResult<Option<Song>>;
    async fn save_song(&self, song: &Song) -> StorageResult<()>;
    async fn delete_song(&self, id: &SongId) -> StorageResult<()>;
    async fn load_variant(
        &self,
        song_id: &SongId,
        variant_id: &SceneId,
    ) -> StorageResult<Option<Scene>>;
}

// endregion: --- Trait

// region: --- SongRepoLive

#[derive(Clone)]
pub struct SongRepoLive {
    db: DatabaseConnection,
}

impl SongRepoLive {
    #[must_use]
    pub const fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Initialize the database schema.
    ///
    /// # Errors
    ///
    /// Returns an error if the database schema cannot be created.
    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut songs = schema.create_table_from_entity(entity::song::Entity);
        songs.if_not_exists();
        self.db.execute(backend.build(&songs)).await?;

        let mut sections = schema.create_table_from_entity(entity::scene::Entity);
        sections.if_not_exists();
        self.db.execute(backend.build(&sections)).await?;

        Ok(())
    }

    fn variant_state_to_json(section: &Scene) -> StorageResult<String> {
        let state = SectionState {
            source: &section.source,
            overrides: &section.overrides,
        };
        serde_json::to_string(&state)
            .map_err(|e| StorageError::Data(format!("failed to serialize section: {e}")))
    }

    fn variant_state_from_json(json: &str) -> StorageResult<SectionStateOwned> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse section json: {e}")))
    }

    fn metadata_to_json(metadata: &Metadata) -> StorageResult<String> {
        serde_json::to_string(metadata)
            .map_err(|e| StorageError::Data(format!("failed to serialize metadata: {e}")))
    }

    fn metadata_from_json(json: &str) -> StorageResult<Metadata> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse metadata json: {e}")))
    }

    fn variant_from_model(model: &entity::scene::Model) -> StorageResult<Scene> {
        let state = Self::variant_state_from_json(&model.state_json)?;
        let metadata = Self::metadata_from_json(&model.metadata_json)?;
        Ok(Scene {
            id: model.variant_id_branded(),
            name: model.name.clone(),
            source: state.source,
            overrides: state.overrides,
            metadata,
        })
    }

    async fn assemble_song(&self, model: &entity::song::Model) -> StorageResult<Song> {
        let variant_models = entity::scene::Entity::find()
            .filter(entity::scene::Column::SongId.eq(model.id.clone()))
            .order_by_asc(entity::scene::Column::Position)
            .all(&self.db)
            .await?;

        let mut sections = Vec::with_capacity(variant_models.len());
        for vm in &variant_models {
            sections.push(Self::variant_from_model(vm)?);
        }

        let metadata = Self::metadata_from_json(&model.metadata_json)?;

        Ok(Song {
            id: model.song_id_branded(),
            name: model.name.clone(),
            artist: model.artist.clone(),
            default_scene_id: model.default_variant_id_branded(),
            scenes: sections,
            metadata,
        })
    }
}

// endregion: --- SongRepoLive

// region: --- Serialization types

#[derive(serde::Serialize)]
struct SectionState<'a> {
    source: &'a SceneSource,
    overrides: &'a [Override],
}

#[derive(serde::Deserialize)]
struct SectionStateOwned {
    source: SceneSource,
    overrides: Vec<Override>,
}

// endregion: --- Serialization types

// region: --- Trait impl

#[async_trait::async_trait]
impl SongRepo for SongRepoLive {
    async fn list_songs(&self) -> StorageResult<Vec<Song>> {
        let models = entity::song::Entity::find()
            .order_by_asc(entity::song::Column::Id)
            .all(&self.db)
            .await?;

        let mut out = Vec::with_capacity(models.len());
        for model in &models {
            out.push(self.assemble_song(model).await?);
        }
        Ok(out)
    }

    async fn load_song(&self, id: &SongId) -> StorageResult<Option<Song>> {
        let model = entity::song::Entity::find_by_id(id.as_str().to_string())
            .one(&self.db)
            .await?;
        match model {
            Some(ref m) => Ok(Some(self.assemble_song(m).await?)),
            None => Ok(None),
        }
    }

    async fn save_song(&self, song: &Song) -> StorageResult<()> {
        entity::song::Entity::delete_by_id(song.id.as_str().to_string())
            .exec(&self.db)
            .await
            .ok();

        entity::song::Entity::insert(entity::song::ActiveModel {
            id: Set(song.id.as_str().to_string()),
            name: Set(song.name.clone()),
            artist: Set(song.artist.clone()),
            default_variant_id: Set(song.default_scene_id.as_str().to_string()),
            metadata_json: Set(Self::metadata_to_json(&song.metadata)?),
        })
        .exec(&self.db)
        .await?;

        for (position, section) in song.scenes.iter().enumerate() {
            entity::scene::Entity::insert(entity::scene::ActiveModel {
                id: Set(section.id.as_str().to_string()),
                song_id: Set(song.id.as_str().to_string()),
                position: Set(position as i32),
                name: Set(section.name.clone()),
                state_json: Set(Self::variant_state_to_json(section)?),
                metadata_json: Set(Self::metadata_to_json(&section.metadata)?),
            })
            .exec(&self.db)
            .await?;
        }

        Ok(())
    }

    async fn delete_song(&self, id: &SongId) -> StorageResult<()> {
        entity::song::Entity::delete_by_id(id.as_str().to_string())
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn load_variant(
        &self,
        song_id: &SongId,
        variant_id: &SceneId,
    ) -> StorageResult<Option<Scene>> {
        let model = entity::scene::Entity::find_by_id(variant_id.as_str().to_string())
            .filter(entity::scene::Column::SongId.eq(song_id.as_str().to_string()))
            .one(&self.db)
            .await?;

        match model {
            Some(ref m) => Ok(Some(Self::variant_from_model(m)?)),
            None => Ok(None),
        }
    }
}

// endregion: --- Trait impl

// region: --- Tests

// endregion: --- Tests
