//! Song repository — data access for Song collections and Section variants.

use sea_orm::*;
use signal_proto::metadata::Metadata;
use signal_proto::overrides::Override;
use signal_proto::song::{Section, SectionId, SectionSource, Song, SongId};

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait SongRepo: Send + Sync {
    async fn list_songs(&self) -> StorageResult<Vec<Song>>;
    async fn load_song(&self, id: &SongId) -> StorageResult<Option<Song>>;
    async fn save_song(&self, song: &Song) -> StorageResult<()>;
    async fn delete_song(&self, id: &SongId) -> StorageResult<()>;
    async fn load_variant(
        &self,
        song_id: &SongId,
        variant_id: &SectionId,
    ) -> StorageResult<Option<Section>>;
}

// endregion: --- Trait

// region: --- SongRepoLive

#[derive(Clone)]
pub struct SongRepoLive {
    db: DatabaseConnection,
}

impl SongRepoLive {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut songs = schema.create_table_from_entity(entity::song::Entity);
        songs.if_not_exists();
        self.db.execute(backend.build(&songs)).await?;

        let mut sections = schema.create_table_from_entity(entity::section::Entity);
        sections.if_not_exists();
        self.db.execute(backend.build(&sections)).await?;

        Ok(())
    }

    fn variant_state_to_json(section: &Section) -> StorageResult<String> {
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

    fn variant_from_model(model: &entity::section::Model) -> StorageResult<Section> {
        let state = Self::variant_state_from_json(&model.state_json)?;
        let metadata = Self::metadata_from_json(&model.metadata_json)?;
        Ok(Section {
            id: model.variant_id_branded(),
            name: model.name.clone(),
            source: state.source,
            overrides: state.overrides,
            metadata,
        })
    }

    async fn assemble_song(&self, model: &entity::song::Model) -> StorageResult<Song> {
        let variant_models = entity::section::Entity::find()
            .filter(entity::section::Column::SongId.eq(model.id.clone()))
            .order_by_asc(entity::section::Column::Position)
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
            default_section_id: model.default_variant_id_branded(),
            sections,
            metadata,
        })
    }
}

// endregion: --- SongRepoLive

// region: --- Serialization types

#[derive(serde::Serialize)]
struct SectionState<'a> {
    source: &'a SectionSource,
    overrides: &'a [Override],
}

#[derive(serde::Deserialize)]
struct SectionStateOwned {
    source: SectionSource,
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
            default_variant_id: Set(song.default_section_id.as_str().to_string()),
            metadata_json: Set(Self::metadata_to_json(&song.metadata)?),
        })
        .exec(&self.db)
        .await?;

        for (position, section) in song.sections.iter().enumerate() {
            entity::section::Entity::insert(entity::section::ActiveModel {
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
        variant_id: &SectionId,
    ) -> StorageResult<Option<Section>> {
        let model = entity::section::Entity::find_by_id(variant_id.as_str().to_string())
            .filter(entity::section::Column::SongId.eq(song_id.as_str().to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use signal_proto::seed_id;

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    fn soid(name: &str) -> SongId {
        SongId::from_uuid(seed_id(name))
    }
    fn secid(name: &str) -> SectionId {
        SectionId::from_uuid(seed_id(name))
    }

    async fn test_repo() -> Result<SongRepoLive> {
        let db = Database::connect("sqlite::memory:").await?;
        let repo = SongRepoLive::new(db);
        repo.init_schema().await?;
        Ok(repo)
    }

    fn sample_song() -> Song {
        let verse = Section::from_patch(seed_id("sec-verse"), "Verse", seed_id("patch-clean"));
        let chorus = Section::from_patch(seed_id("sec-chorus"), "Chorus", seed_id("patch-lead"));

        let mut song =
            Song::new(seed_id("song-1"), "Amazing Grace", verse).with_artist("Traditional");
        song.add_section(chorus);
        song
    }

    #[tokio::test]
    async fn save_load_round_trip() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_song(&sample_song()).await?;

        let loaded = repo.load_song(&soid("song-1")).await?;
        let loaded = loaded.expect("should find song");
        assert_eq!(loaded.name, "Amazing Grace");
        assert_eq!(loaded.artist.as_deref(), Some("Traditional"));
        assert_eq!(loaded.sections.len(), 2);
        assert_eq!(loaded.default_section_id, secid("sec-verse"));
        Ok(())
    }

    #[tokio::test]
    async fn list_songs_returns_all() -> Result<()> {
        let repo = test_repo().await?;
        let s1 = Song::new(
            seed_id("s1"),
            "Song 1",
            Section::from_patch(seed_id("sec1"), "Verse", seed_id("p1")),
        );
        let s2 = Song::new(
            seed_id("s2"),
            "Song 2",
            Section::from_patch(seed_id("sec2"), "Verse", seed_id("p2")),
        );
        repo.save_song(&s1).await?;
        repo.save_song(&s2).await?;

        let songs = repo.list_songs().await?;
        assert_eq!(songs.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn load_missing_returns_none() -> Result<()> {
        let repo = test_repo().await?;
        let loaded = repo.load_song(&soid("nonexistent")).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn delete_song_removes_it() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_song(&sample_song()).await?;
        repo.delete_song(&soid("song-1")).await?;
        let loaded = repo.load_song(&soid("song-1")).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn load_variant_by_id() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_song(&sample_song()).await?;

        let variant = repo
            .load_variant(&soid("song-1"), &secid("sec-chorus"))
            .await?;
        let variant = variant.expect("should find section");
        assert_eq!(variant.name, "Chorus");
        match &variant.source {
            SectionSource::Patch { patch_id } => {
                assert_eq!(
                    *patch_id,
                    signal_proto::profile::PatchId::from_uuid(seed_id("patch-lead"))
                );
            }
            _ => panic!("expected Patch source"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn rig_scene_source_round_trip() -> Result<()> {
        let repo = test_repo().await?;
        let section = Section::from_rig_scene(
            seed_id("sec-1"),
            "Intro",
            seed_id("rig-1"),
            seed_id("rs-ambient"),
        );
        let song = Song::new(seed_id("song-2"), "Instrumental", section);
        repo.save_song(&song).await?;

        let loaded = repo.load_song(&soid("song-2")).await?.unwrap();
        let sec = &loaded.sections[0];
        match &sec.source {
            SectionSource::RigScene { rig_id, scene_id } => {
                assert_eq!(
                    *rig_id,
                    signal_proto::rig::RigId::from_uuid(seed_id("rig-1"))
                );
                assert_eq!(
                    *scene_id,
                    signal_proto::rig::RigSceneId::from_uuid(seed_id("rs-ambient"))
                );
            }
            _ => panic!("expected RigScene source"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn artist_none_round_trip() -> Result<()> {
        let repo = test_repo().await?;
        let song = Song::new(
            seed_id("song-3"),
            "Untitled",
            Section::from_patch(seed_id("s1"), "Main", seed_id("p1")),
        );
        repo.save_song(&song).await?;

        let loaded = repo.load_song(&soid("song-3")).await?.unwrap();
        assert!(loaded.artist.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn save_overwrites_existing() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_song(&sample_song()).await?;

        let updated = Song::new(
            seed_id("song-1"),
            "Renamed Song",
            Section::from_patch(seed_id("sec-only"), "Only Section", seed_id("p1")),
        );
        repo.save_song(&updated).await?;

        let loaded = repo.load_song(&soid("song-1")).await?.unwrap();
        assert_eq!(loaded.name, "Renamed Song");
        assert_eq!(loaded.sections.len(), 1);
        assert!(loaded.artist.is_none());
        Ok(())
    }
}

// endregion: --- Tests
