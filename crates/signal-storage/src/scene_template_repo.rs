//! Scene template repository — data access for standalone scene templates.

use sea_orm::prelude::Expr;
use sea_orm::*;
use signal_proto::metadata::Metadata;
use signal_proto::overrides::Override;
use signal_proto::rig::EngineSelection;
use signal_proto::scene_template::{SceneTemplate, SceneTemplateId};

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait SceneTemplateRepo: Send + Sync {
    async fn list_scene_templates(&self) -> StorageResult<Vec<SceneTemplate>>;
    async fn load_scene_template(
        &self,
        id: &SceneTemplateId,
    ) -> StorageResult<Option<SceneTemplate>>;
    async fn save_scene_template(&self, template: &SceneTemplate) -> StorageResult<()>;
    async fn delete_scene_template(&self, id: &SceneTemplateId) -> StorageResult<()>;
    async fn reorder_scene_templates(&self, ordered_ids: &[SceneTemplateId]) -> StorageResult<()>;
}

// endregion: --- Trait

// region: --- SceneTemplateRepoLive

#[derive(Clone)]
pub struct SceneTemplateRepoLive {
    db: DatabaseConnection,
}

impl SceneTemplateRepoLive {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut stmt = schema.create_table_from_entity(entity::scene_template::Entity);
        stmt.if_not_exists();
        self.db.execute(backend.build(&stmt)).await?;

        // Migration: add sort_order column for existing DBs.
        self.db
            .execute_unprepared(
                "ALTER TABLE scene_templates ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
            )
            .await
            .ok();

        Ok(())
    }

    fn state_to_json(template: &SceneTemplate) -> StorageResult<String> {
        let state = SceneTemplateState {
            engine_selections: &template.engine_selections,
            overrides: &template.overrides,
        };
        serde_json::to_string(&state)
            .map_err(|e| StorageError::Data(format!("failed to serialize scene template: {e}")))
    }

    fn state_from_json(json: &str) -> StorageResult<SceneTemplateStateOwned> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse scene template json: {e}")))
    }

    fn metadata_to_json(metadata: &Metadata) -> StorageResult<String> {
        serde_json::to_string(metadata)
            .map_err(|e| StorageError::Data(format!("failed to serialize metadata: {e}")))
    }

    fn metadata_from_json(json: &str) -> StorageResult<Metadata> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse metadata json: {e}")))
    }

    fn from_model(model: &entity::scene_template::Model) -> StorageResult<SceneTemplate> {
        let state = Self::state_from_json(&model.state_json)?;
        let metadata = Self::metadata_from_json(&model.metadata_json)?;
        Ok(SceneTemplate {
            id: model.id_branded(),
            name: model.name.clone(),
            engine_selections: state.engine_selections,
            overrides: state.overrides,
            metadata,
        })
    }
}

// endregion: --- SceneTemplateRepoLive

// region: --- Serialization types

#[derive(serde::Serialize)]
struct SceneTemplateState<'a> {
    engine_selections: &'a [EngineSelection],
    overrides: &'a [Override],
}

#[derive(serde::Deserialize)]
struct SceneTemplateStateOwned {
    engine_selections: Vec<EngineSelection>,
    overrides: Vec<Override>,
}

// endregion: --- Serialization types

// region: --- Trait impl

#[async_trait::async_trait]
impl SceneTemplateRepo for SceneTemplateRepoLive {
    async fn list_scene_templates(&self) -> StorageResult<Vec<SceneTemplate>> {
        let models = entity::scene_template::Entity::find()
            .order_by_asc(entity::scene_template::Column::SortOrder)
            .order_by_asc(entity::scene_template::Column::Id)
            .all(&self.db)
            .await?;

        models.iter().map(Self::from_model).collect()
    }

    async fn load_scene_template(
        &self,
        id: &SceneTemplateId,
    ) -> StorageResult<Option<SceneTemplate>> {
        let model = entity::scene_template::Entity::find_by_id(id.as_str().to_string())
            .one(&self.db)
            .await?;
        match model {
            Some(ref m) => Ok(Some(Self::from_model(m)?)),
            None => Ok(None),
        }
    }

    async fn save_scene_template(&self, template: &SceneTemplate) -> StorageResult<()> {
        // Get current sort_order if updating, or assign next order for new entries.
        let sort_order =
            match entity::scene_template::Entity::find_by_id(template.id.as_str().to_string())
                .one(&self.db)
                .await?
            {
                Some(existing) => existing.sort_order,
                None => {
                    // Assign next available order.
                    let count = entity::scene_template::Entity::find()
                        .count(&self.db)
                        .await? as i32;
                    count
                }
            };

        entity::scene_template::Entity::delete_by_id(template.id.as_str().to_string())
            .exec(&self.db)
            .await
            .ok();

        entity::scene_template::Entity::insert(entity::scene_template::ActiveModel {
            id: Set(template.id.as_str().to_string()),
            name: Set(template.name.clone()),
            state_json: Set(Self::state_to_json(template)?),
            metadata_json: Set(Self::metadata_to_json(&template.metadata)?),
            sort_order: Set(sort_order),
        })
        .exec(&self.db)
        .await?;

        Ok(())
    }

    async fn delete_scene_template(&self, id: &SceneTemplateId) -> StorageResult<()> {
        entity::scene_template::Entity::delete_by_id(id.as_str().to_string())
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn reorder_scene_templates(&self, ordered_ids: &[SceneTemplateId]) -> StorageResult<()> {
        for (idx, id) in ordered_ids.iter().enumerate() {
            entity::scene_template::Entity::update_many()
                .col_expr(
                    entity::scene_template::Column::SortOrder,
                    Expr::value(idx as i32),
                )
                .filter(entity::scene_template::Column::Id.eq(id.as_str().to_string()))
                .exec(&self.db)
                .await?;
        }
        Ok(())
    }
}

// endregion: --- Trait impl

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use signal_proto::engine::{EngineId, EngineSceneId};
    use signal_proto::rig::EngineSelection;
    use signal_proto::seed_id;

    fn stid(name: &str) -> SceneTemplateId {
        SceneTemplateId::from_uuid(seed_id(name))
    }

    async fn test_repo() -> StorageResult<SceneTemplateRepoLive> {
        let db = Database::connect("sqlite::memory:").await?;
        let repo = SceneTemplateRepoLive::new(db);
        repo.init_schema().await?;
        Ok(repo)
    }

    fn sample_template(name: &str) -> SceneTemplate {
        SceneTemplate::new(stid(name), name).with_engine(EngineSelection::new(
            EngineId::from_uuid(seed_id("eng-1")),
            EngineSceneId::from_uuid(seed_id("es-1")),
        ))
    }

    #[tokio::test]
    async fn save_load_round_trip() {
        let repo = test_repo().await.unwrap();
        let template = sample_template("Clean");
        repo.save_scene_template(&template).await.unwrap();

        let loaded = repo.load_scene_template(&stid("Clean")).await.unwrap();
        let loaded = loaded.expect("should exist");
        assert_eq!(loaded.name, "Clean");
        assert_eq!(loaded.engine_selections.len(), 1);
    }

    #[tokio::test]
    async fn list_returns_all_ordered() {
        let repo = test_repo().await.unwrap();
        repo.save_scene_template(&sample_template("Clean"))
            .await
            .unwrap();
        repo.save_scene_template(&sample_template("Crunch"))
            .await
            .unwrap();
        repo.save_scene_template(&sample_template("Lead"))
            .await
            .unwrap();

        let templates = repo.list_scene_templates().await.unwrap();
        assert_eq!(templates.len(), 3);
    }

    #[tokio::test]
    async fn delete_removes_template() {
        let repo = test_repo().await.unwrap();
        repo.save_scene_template(&sample_template("Clean"))
            .await
            .unwrap();
        repo.delete_scene_template(&stid("Clean")).await.unwrap();

        let loaded = repo.load_scene_template(&stid("Clean")).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn reorder_changes_sort_order() {
        let repo = test_repo().await.unwrap();
        repo.save_scene_template(&sample_template("A"))
            .await
            .unwrap();
        repo.save_scene_template(&sample_template("B"))
            .await
            .unwrap();
        repo.save_scene_template(&sample_template("C"))
            .await
            .unwrap();

        // Reverse the order.
        repo.reorder_scene_templates(&[stid("C"), stid("B"), stid("A")])
            .await
            .unwrap();

        let templates = repo.list_scene_templates().await.unwrap();
        assert_eq!(templates[0].name, "C");
        assert_eq!(templates[1].name, "B");
        assert_eq!(templates[2].name, "A");
    }

    #[tokio::test]
    async fn save_preserves_sort_order_on_update() {
        let repo = test_repo().await.unwrap();
        repo.save_scene_template(&sample_template("Clean"))
            .await
            .unwrap();
        repo.save_scene_template(&sample_template("Lead"))
            .await
            .unwrap();

        // Reorder so Lead is first.
        repo.reorder_scene_templates(&[stid("Lead"), stid("Clean")])
            .await
            .unwrap();

        // Re-save Clean (update name) — should keep its sort_order.
        let mut updated = sample_template("Clean");
        updated.name = "Clean Updated".into();
        repo.save_scene_template(&updated).await.unwrap();

        let templates = repo.list_scene_templates().await.unwrap();
        assert_eq!(templates[0].name, "Lead");
        assert_eq!(templates[1].name, "Clean Updated");
    }
}

// endregion: --- Tests
