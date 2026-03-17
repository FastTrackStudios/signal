//! Browser service implementation — tag inference and faceted search.
//!
//! Implements [`BrowserService`] on [`SignalLive`], providing structured
//! tag extraction from entity names and hierarchical browsing queries.

use super::*;

fn tags_from_name(name: &str) -> TagSet {
    infer_tags_from_name(name)
}

fn add_domain_tag(tags: &mut TagSet, value: &str) {
    tags.insert(StructuredTag::new(TagCategory::DomainLevel, value));
}

fn add_block_type_tag(tags: &mut TagSet, value: &str) {
    tags.insert(StructuredTag::new(TagCategory::Block, value));
}

fn add_module_type_tag(tags: &mut TagSet, value: &str) {
    tags.insert(StructuredTag::new(TagCategory::Module, value));
}

fn add_engine_type_tag(tags: &mut TagSet, value: &str) {
    tags.insert(StructuredTag::new(TagCategory::EngineType, value));
}

fn build_entry(
    kind: BrowserEntityKind,
    id: impl Into<String>,
    name: impl Into<String>,
    tags: TagSet,
    aliases: Vec<String>,
) -> BrowserEntry {
    BrowserEntry {
        node: BrowserNodeId {
            kind,
            id: id.into(),
        },
        name: name.into(),
        tags,
        aliases,
    }
}

impl<B, M, L, E, R, P, So, Se, St, Ra> BrowserService
    for SignalLive<B, M, L, E, R, P, So, Se, St, Ra>
where
    B: BlockRepo,
    M: ModuleRepo,
    L: LayerRepo,
    E: EngineRepo,
    R: RigRepo,
    P: ProfileRepo,
    So: SongRepo,
    Se: SetlistRepo,
    St: SceneTemplateRepo,
    Ra: RackRepo,
{
    async fn browser_index(&self) -> Result<BrowserIndex, SignalServiceError> {
        let mut index = BrowserIndex::default();

        for block_type in ALL_BLOCK_TYPES {
            let collections = self
                .block_repo
                .list_block_collections(*block_type)
                .await
                .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;

            for collection in collections {
                let mut ctags = tags_from_name(collection.name());
                ctags.merge(&TagSet::from_tags(&collection.metadata().tags));
                add_domain_tag(&mut ctags, "block_collection");
                add_block_type_tag(&mut ctags, block_type.as_str());

                index.push(build_entry(
                    BrowserEntityKind::BlockCollection,
                    collection.id().to_string(),
                    collection.name().to_string(),
                    ctags.clone(),
                    vec![block_type.display_name().to_string()],
                ));

                for variant in collection.snapshots() {
                    let mut vtags = tags_from_name(variant.name());
                    vtags.merge(&ctags);
                    vtags.merge(&TagSet::from_tags(&variant.metadata().tags));
                    add_domain_tag(&mut vtags, "block_variant");
                    index.push(build_entry(
                        BrowserEntityKind::BlockVariant,
                        variant.id().to_string(),
                        variant.name().to_string(),
                        vtags,
                        vec![collection.name().to_string()],
                    ));
                }
            }
        }

        let module_collections = self
            .module_repo
            .list_module_collections()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        for collection in module_collections {
            let mut ctags = tags_from_name(collection.name());
            ctags.merge(&TagSet::from_tags(&collection.metadata().tags));
            add_domain_tag(&mut ctags, "module_collection");
            add_module_type_tag(&mut ctags, collection.module_type().as_str());
            index.push(build_entry(
                BrowserEntityKind::ModuleCollection,
                collection.id().to_string(),
                collection.name().to_string(),
                ctags.clone(),
                vec![collection.module_type().display_name().to_string()],
            ));

            for variant in collection.snapshots() {
                let mut vtags = tags_from_name(variant.name());
                vtags.merge(&ctags);
                vtags.merge(&TagSet::from_tags(&variant.metadata().tags));
                add_domain_tag(&mut vtags, "module_variant");
                index.push(build_entry(
                    BrowserEntityKind::ModuleVariant,
                    variant.id().to_string(),
                    variant.name().to_string(),
                    vtags,
                    vec![collection.name().to_string()],
                ));
            }
        }

        let layers = self
            .layer_repo
            .list_layers()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        for layer in layers {
            let mut ctags = tags_from_name(&layer.name);
            ctags.merge(&TagSet::from_tags(&layer.metadata.tags));
            add_domain_tag(&mut ctags, "layer_collection");
            add_engine_type_tag(&mut ctags, layer.engine_type.as_str());
            index.push(build_entry(
                BrowserEntityKind::LayerCollection,
                layer.id.to_string(),
                layer.name.clone(),
                ctags.clone(),
                vec![layer.engine_type.as_str().to_string()],
            ));

            for variant in &layer.variants {
                let mut vtags = tags_from_name(&variant.name);
                vtags.merge(&ctags);
                vtags.merge(&TagSet::from_tags(&variant.metadata.tags));
                add_domain_tag(&mut vtags, "layer_variant");
                index.push(build_entry(
                    BrowserEntityKind::LayerVariant,
                    variant.id.to_string(),
                    variant.name.clone(),
                    vtags,
                    vec![layer.name.clone()],
                ));
            }
        }

        let engines = self
            .engine_repo
            .list_engines()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        for engine in engines {
            let mut ctags = tags_from_name(&engine.name);
            ctags.merge(&TagSet::from_tags(&engine.metadata.tags));
            add_domain_tag(&mut ctags, "engine_collection");
            add_engine_type_tag(&mut ctags, engine.engine_type.as_str());
            index.push(build_entry(
                BrowserEntityKind::EngineCollection,
                engine.id.to_string(),
                engine.name.clone(),
                ctags.clone(),
                vec![engine.engine_type.as_str().to_string()],
            ));

            for variant in &engine.variants {
                let mut vtags = tags_from_name(&variant.name);
                vtags.merge(&ctags);
                vtags.merge(&TagSet::from_tags(&variant.metadata.tags));
                add_domain_tag(&mut vtags, "engine_variant");
                index.push(build_entry(
                    BrowserEntityKind::EngineVariant,
                    variant.id.to_string(),
                    variant.name.clone(),
                    vtags,
                    vec![engine.name.clone()],
                ));
            }
        }

        let rigs = self
            .rig_repo
            .list_rigs()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        for rig in rigs {
            let mut ctags = tags_from_name(&rig.name);
            ctags.merge(&TagSet::from_tags(&rig.metadata.tags));
            add_domain_tag(&mut ctags, "rig_collection");
            if let Some(rig_type) = rig.rig_type {
                ctags.insert(StructuredTag::new(TagCategory::RigType, rig_type.as_str()));
            }
            index.push(build_entry(
                BrowserEntityKind::RigCollection,
                rig.id.to_string(),
                rig.name.clone(),
                ctags.clone(),
                vec![],
            ));

            for variant in &rig.variants {
                let mut vtags = tags_from_name(&variant.name);
                vtags.merge(&ctags);
                vtags.merge(&TagSet::from_tags(&variant.metadata.tags));
                add_domain_tag(&mut vtags, "rig_variant");
                index.push(build_entry(
                    BrowserEntityKind::RigVariant,
                    variant.id.to_string(),
                    variant.name.clone(),
                    vtags,
                    vec![rig.name.clone()],
                ));
            }
        }

        let profiles = self
            .profile_repo
            .list_profiles()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        for profile in profiles {
            let mut ctags = tags_from_name(&profile.name);
            ctags.merge(&TagSet::from_tags(&profile.metadata.tags));
            add_domain_tag(&mut ctags, "profile_collection");
            index.push(build_entry(
                BrowserEntityKind::ProfileCollection,
                profile.id.to_string(),
                profile.name.clone(),
                ctags.clone(),
                vec![],
            ));

            for variant in &profile.patches {
                let mut vtags = tags_from_name(&variant.name);
                vtags.merge(&ctags);
                vtags.merge(&TagSet::from_tags(&variant.metadata.tags));
                add_domain_tag(&mut vtags, "profile_variant");
                index.push(build_entry(
                    BrowserEntityKind::ProfileVariant,
                    variant.id.to_string(),
                    variant.name.clone(),
                    vtags,
                    vec![profile.name.clone()],
                ));
            }
        }

        let songs = self
            .song_repo
            .list_songs()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        for song in songs {
            let mut ctags = tags_from_name(&song.name);
            ctags.merge(&TagSet::from_tags(&song.metadata.tags));
            add_domain_tag(&mut ctags, "song_collection");
            if let Some(artist) = &song.artist {
                ctags.insert(StructuredTag::new(TagCategory::Custom, artist));
            }
            index.push(build_entry(
                BrowserEntityKind::SongCollection,
                song.id.to_string(),
                song.name.clone(),
                ctags.clone(),
                song.artist.clone().into_iter().collect(),
            ));

            for variant in &song.sections {
                let mut vtags = tags_from_name(&variant.name);
                vtags.merge(&ctags);
                vtags.merge(&TagSet::from_tags(&variant.metadata.tags));
                add_domain_tag(&mut vtags, "song_variant");
                index.push(build_entry(
                    BrowserEntityKind::SongVariant,
                    variant.id.to_string(),
                    variant.name.clone(),
                    vtags,
                    vec![song.name.clone()],
                ));
            }
        }

        let setlists = self
            .setlist_repo
            .list_setlists()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        for setlist in setlists {
            let mut ctags = tags_from_name(&setlist.name);
            ctags.merge(&TagSet::from_tags(&setlist.metadata.tags));
            add_domain_tag(&mut ctags, "setlist_collection");
            index.push(build_entry(
                BrowserEntityKind::SetlistCollection,
                setlist.id.to_string(),
                setlist.name.clone(),
                ctags.clone(),
                vec![],
            ));

            for variant in &setlist.entries {
                let mut vtags = tags_from_name(&variant.name);
                vtags.merge(&ctags);
                vtags.merge(&TagSet::from_tags(&variant.metadata.tags));
                add_domain_tag(&mut vtags, "setlist_variant");
                index.push(build_entry(
                    BrowserEntityKind::SetlistVariant,
                    variant.id.to_string(),
                    variant.name.clone(),
                    vtags,
                    vec![setlist.name.clone()],
                ));
            }
        }

        Ok(index)
    }

    async fn browse(&self, query: BrowserQuery) -> Result<Vec<BrowserHit>, SignalServiceError> {
        let index: BrowserIndex = BrowserService::browser_index(self).await?;
        Ok(index.query(&query, &TagWeights::default()))
    }
}
