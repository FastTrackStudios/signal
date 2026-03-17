//! Resolve service implementation — compiles variant selections into a resolved graph.
//!
//! Implements [`ResolveService`] on [`SignalLive`], walking the hierarchy from
//! a resolve target (rig scene, profile patch, or song section) down to concrete
//! block parameter values with the override stack applied.

use super::*;

fn apply_block_parameter_overrides(block: &mut Block, overrides: &[BlockParameterOverride]) {
    for ov in overrides {
        if let Some((idx, _)) = block
            .parameters()
            .iter()
            .enumerate()
            .find(|(_, p)| p.id() == ov.parameter_id())
        {
            block.set_parameter_value(idx, ov.value().get());
        }
    }
}

fn merge_override_levels(
    levels: &[Vec<signal_proto::overrides::Override>],
) -> Vec<signal_proto::overrides::Override> {
    // nearest-scope-wins: later levels replace earlier path entries
    let mut by_path: HashMap<String, signal_proto::overrides::Override> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for level in levels {
        for ov in level {
            let key = ov.path.as_str();
            if !by_path.contains_key(&key) {
                order.push(key.clone());
            }
            by_path.insert(key, ov.clone());
        }
    }
    order
        .into_iter()
        .filter_map(|k| by_path.remove(&k))
        .collect()
}

fn map_policy_err(
    scope: &str,
    err: signal_proto::override_policy::OverridePolicyError,
) -> ResolveError {
    ResolveError::InvalidReference(format!("{scope} override policy violation: {err:?}"))
}

fn normalize_ref_id(raw: &str) -> String {
    let looks_like_uuid = raw.len() == 36
        && [8, 13, 18, 23]
            .into_iter()
            .all(|i| raw.as_bytes()[i] == b'-')
        && raw
            .bytes()
            .enumerate()
            .all(|(i, b)| [8, 13, 18, 23].contains(&i) || b.is_ascii_hexdigit());
    if looks_like_uuid {
        raw.to_string()
    } else {
        signal_proto::seed_id(raw).to_string()
    }
}

fn id_matches(entity_id: &str, path_or_alias: &str) -> bool {
    entity_id == path_or_alias || entity_id == normalize_ref_id(path_or_alias)
}

fn segment_engine(path: &signal_proto::overrides::NodePath) -> Option<&str> {
    path.segments().iter().find_map(|seg| match seg {
        NodePathSegment::Engine(v) => Some(v.as_str()),
        _ => None,
    })
}

fn segment_layer(path: &signal_proto::overrides::NodePath) -> Option<&str> {
    path.segments().iter().find_map(|seg| match seg {
        NodePathSegment::Layer(v) => Some(v.as_str()),
        _ => None,
    })
}

fn segment_module(path: &signal_proto::overrides::NodePath) -> Option<&str> {
    path.segments().iter().find_map(|seg| match seg {
        NodePathSegment::Module(v) => Some(v.as_str()),
        _ => None,
    })
}

fn segment_block(path: &signal_proto::overrides::NodePath) -> Option<&str> {
    path.segments().iter().find_map(|seg| match seg {
        NodePathSegment::Block(v) => Some(v.as_str()),
        _ => None,
    })
}

fn segment_param(path: &signal_proto::overrides::NodePath) -> Option<&str> {
    path.segments().iter().find_map(|seg| match seg {
        NodePathSegment::Parameter(v) => Some(v.as_str()),
        _ => None,
    })
}

fn apply_effective_set_overrides(graph: &mut ResolvedGraph) {
    for ov in &graph.effective_overrides {
        let NodeOverrideOp::Set(value) = &ov.op else {
            continue;
        };

        let engine_id = segment_engine(&ov.path);
        let layer_id = segment_layer(&ov.path);
        let module_id = segment_module(&ov.path);
        let block_id = segment_block(&ov.path);
        let Some(parameter_id) = segment_param(&ov.path) else {
            continue;
        };

        for engine in &mut graph.engines {
            if let Some(expected) = engine_id {
                if !id_matches(engine.engine_id.as_str(), expected) {
                    continue;
                }
            }
            for layer in &mut engine.layers {
                if let Some(expected) = layer_id {
                    if !id_matches(layer.layer_id.as_str(), expected) {
                        continue;
                    }
                }

                for module in &mut layer.modules {
                    if let Some(expected) = module_id {
                        if !id_matches(module.source_preset_id.as_str(), expected) {
                            continue;
                        }
                    }
                    for rb in &mut module.blocks {
                        if let Some(expected) = block_id {
                            if !id_matches(&rb.node_id, expected) && rb.node_id != expected {
                                continue;
                            }
                        }
                        if let Some((idx, _)) = rb
                            .block
                            .parameters()
                            .iter()
                            .enumerate()
                            .find(|(_, p)| p.id() == parameter_id)
                        {
                            rb.block.set_parameter_value(idx, value.get());
                        }
                    }
                }

                for rb in &mut layer.standalone_blocks {
                    if let Some(expected) = block_id {
                        if !id_matches(&rb.node_id, expected) && rb.node_id != expected {
                            continue;
                        }
                    }
                    if let Some((idx, _)) = rb
                        .block
                        .parameters()
                        .iter()
                        .enumerate()
                        .find(|(_, p)| p.id() == parameter_id)
                    {
                        rb.block.set_parameter_value(idx, value.get());
                    }
                }
            }
        }
    }
}

impl<B, M, L, E, R, P, So, Se, St, Ra> SignalLive<B, M, L, E, R, P, So, Se, St, Ra>
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
    async fn resolve_block_ref(
        &self,
        block_type: BlockType,
        preset_id: &PresetId,
        snapshot_id: Option<&SnapshotId>,
        node_id: String,
        label: String,
        saved_at_version: Option<u32>,
    ) -> Result<ResolvedBlock, ResolveError> {
        let snap = match snapshot_id {
            Some(variant_id) => self
                .block_repo
                .load_block_variant(block_type, preset_id, variant_id)
                .await
                .map_err(|e| ResolveError::NotFound(format!("block variant load failed: {e}")))?,
            None => self
                .block_repo
                .load_block_default_variant(block_type, preset_id)
                .await
                .map_err(|e| {
                    ResolveError::NotFound(format!("block default variant load failed: {e}"))
                })?,
        }
        .ok_or_else(|| {
            ResolveError::InvalidReference(match snapshot_id {
                Some(variant_id) => format!(
                    "missing block variant: type={} preset={} variant={}",
                    block_type.as_str(),
                    preset_id,
                    variant_id
                ),
                None => format!(
                    "missing block default variant: type={} preset={}",
                    block_type.as_str(),
                    preset_id
                ),
            })
        })?;

        let stale = match saved_at_version {
            Some(saved) => saved < snap.version(),
            None => false, // unknown/legacy — not flagged
        };

        Ok(ResolvedBlock {
            node_id,
            label,
            block_type,
            source_preset_id: Some(preset_id.clone()),
            source_variant_id: Some(snap.id().clone()),
            block: snap.block(),
            state_data: snap.state_data().map(|d| d.to_vec()),
            stale,
        })
    }

    async fn resolve_standalone_block_ref(
        &self,
        preset_id: &PresetId,
        snapshot_id: Option<&SnapshotId>,
        node_id: String,
        label: String,
    ) -> Result<ResolvedBlock, ResolveError> {
        for block_type in ALL_BLOCK_TYPES {
            let resolved = self
                .resolve_block_ref(
                    *block_type,
                    preset_id,
                    snapshot_id,
                    node_id.clone(),
                    label.clone(),
                    None, // standalone refs don't track saved version
                )
                .await;
            if let Ok(resolved) = resolved {
                return Ok(resolved);
            }
        }
        Err(ResolveError::InvalidReference(match snapshot_id {
            Some(variant_id) => format!(
                "standalone block variant not found for any block type: preset={} variant={}",
                preset_id, variant_id
            ),
            None => format!(
                "standalone block default variant not found for any block type: preset={}",
                preset_id
            ),
        }))
    }

    async fn resolve_module_snapshot(
        &self,
        snapshot: &ModuleSnapshot,
    ) -> Result<ResolvedModule, ResolveError> {
        let mut blocks = Vec::new();
        for block in snapshot.module().blocks() {
            let saved_ver = block.source().saved_at_version();
            let mut resolved = match block.source() {
                ModuleBlockSource::PresetDefault { preset_id, .. } => {
                    self.resolve_block_ref(
                        block.block_type(),
                        preset_id,
                        None,
                        block.id().to_string(),
                        block.label().to_string(),
                        saved_ver,
                    )
                    .await?
                }
                ModuleBlockSource::PresetSnapshot {
                    preset_id,
                    snapshot_id,
                    ..
                } => {
                    self.resolve_block_ref(
                        block.block_type(),
                        preset_id,
                        Some(snapshot_id),
                        block.id().to_string(),
                        block.label().to_string(),
                        saved_ver,
                    )
                    .await?
                }
                ModuleBlockSource::Inline { block: inline } => ResolvedBlock {
                    node_id: block.id().to_string(),
                    label: block.label().to_string(),
                    block_type: block.block_type(),
                    source_preset_id: None,
                    source_variant_id: None,
                    block: inline.clone(),
                    state_data: None,
                    stale: false,
                },
            };
            apply_block_parameter_overrides(&mut resolved.block, block.overrides());
            blocks.push(resolved);
        }
        Ok(ResolvedModule {
            source_preset_id: ModulePresetId::new(),
            source_variant_id: snapshot.id().clone(),
            blocks,
        })
    }

    async fn resolve_module_ref(
        &self,
        preset_id: &ModulePresetId,
        variant_id: Option<&ModuleSnapshotId>,
    ) -> Result<ResolvedModule, ResolveError> {
        let snapshot = match variant_id {
            Some(variant_id) => self
                .module_repo
                .load_module_variant(preset_id, variant_id)
                .await
                .map_err(|e| ResolveError::NotFound(format!("module variant load failed: {e}")))?,
            None => self
                .module_repo
                .load_module_default_variant(preset_id)
                .await
                .map_err(|e| {
                    ResolveError::NotFound(format!("module default variant load failed: {e}"))
                })?,
        }
        .ok_or_else(|| {
            ResolveError::InvalidReference(match variant_id {
                Some(variant_id) => format!(
                    "missing module variant: preset={} variant={}",
                    preset_id, variant_id
                ),
                None => format!("missing module default variant: preset={preset_id}"),
            })
        })?;
        let mut resolved = self.resolve_module_snapshot(&snapshot).await?;
        resolved.source_preset_id = preset_id.clone();
        resolved.source_variant_id = snapshot.id().clone();
        Ok(resolved)
    }

    async fn resolve_layer_tree(
        &self,
        engine_id: &EngineId,
        start_layer_id: LayerId,
        start_variant_id: LayerSnapshotId,
        start_source: LayerSource,
        selection_overrides: &[signal_proto::overrides::Override],
    ) -> Result<Vec<ResolvedLayer>, ResolveError> {
        #[derive(Clone)]
        enum Phase {
            Explore,
            Build,
        }
        #[derive(Clone)]
        struct Frame {
            layer_id: LayerId,
            variant_id: LayerSnapshotId,
            source: LayerSource,
            phase: Phase,
        }

        let mut stack = vec![Frame {
            layer_id: start_layer_id,
            variant_id: start_variant_id,
            source: start_source,
            phase: Phase::Explore,
        }];
        let mut active: HashSet<String> = HashSet::new();
        let mut loaded: HashMap<String, (Layer, LayerSnapshot, LayerSource)> = HashMap::new();
        let mut resolved = Vec::new();

        while let Some(frame) = stack.pop() {
            let key = format!("{}::{}", frame.layer_id, frame.variant_id);
            match frame.phase {
                Phase::Explore => {
                    if active.contains(&key) {
                        return Err(ResolveError::CycleDetected(format!(
                            "layer variant cycle at {key}"
                        )));
                    }
                    active.insert(key.clone());
                    let layer = self
                        .layer_repo
                        .load_layer(&frame.layer_id)
                        .await
                        .map_err(|e| ResolveError::NotFound(format!("layer load failed: {e}")))?
                        .ok_or_else(|| {
                            ResolveError::NotFound(format!("layer not found: {}", frame.layer_id))
                        })?;
                    let variant = layer.variant(&frame.variant_id).cloned().ok_or_else(|| {
                        ResolveError::NotFound(format!(
                            "layer variant not found: {}::{}",
                            frame.layer_id, frame.variant_id
                        ))
                    })?;
                    validate_overrides::<SnapshotPolicy>(&variant.overrides)
                        .map_err(|e| map_policy_err("layer snapshot", e))?;
                    loaded.insert(
                        key.clone(),
                        (layer.clone(), variant.clone(), frame.source.clone()),
                    );
                    stack.push(Frame {
                        layer_id: frame.layer_id,
                        variant_id: frame.variant_id,
                        source: frame.source,
                        phase: Phase::Build,
                    });
                    for layer_ref in variant.layer_refs.iter().rev() {
                        let child_layer = self
                            .layer_repo
                            .load_layer(&layer_ref.collection_id)
                            .await
                            .map_err(|e| ResolveError::NotFound(format!("layer load failed: {e}")))?
                            .ok_or_else(|| {
                                ResolveError::InvalidReference(format!(
                                    "missing layer ref: {}",
                                    layer_ref.collection_id
                                ))
                            })?;
                        let child_variant_id = layer_ref
                            .variant_id
                            .clone()
                            .unwrap_or_else(|| child_layer.default_variant_id.clone());
                        stack.push(Frame {
                            layer_id: child_layer.id.clone(),
                            variant_id: child_variant_id,
                            source: LayerSource::InlinedInParent,
                            phase: Phase::Explore,
                        });
                    }
                }
                Phase::Build => {
                    active.remove(&key);
                    let (layer, variant, source) = loaded.remove(&key).ok_or_else(|| {
                        ResolveError::InvalidReference(format!("missing loaded frame {key}"))
                    })?;

                    let mut module_refs = variant.module_refs.clone();
                    let mut block_refs = variant.block_refs.clone();
                    let mut disabled_module_ids: HashSet<String> = HashSet::new();
                    let mut disabled_block_ids: HashSet<String> = HashSet::new();

                    for ov in selection_overrides {
                        if let Some(seg_engine) = segment_engine(&ov.path) {
                            if !id_matches(engine_id.as_str(), seg_engine) {
                                continue;
                            }
                        }
                        if let Some(seg_layer) = segment_layer(&ov.path) {
                            if !id_matches(layer.id.as_str(), seg_layer) {
                                continue;
                            }
                        } else {
                            continue;
                        }

                        if let Some(seg_module) = segment_module(&ov.path) {
                            if let Some(mr) = module_refs
                                .iter_mut()
                                .find(|mr| id_matches(mr.collection_id.as_str(), seg_module))
                            {
                                match &ov.op {
                                    NodeOverrideOp::ReplaceRef(next) => {
                                        let next_variant =
                                            ModuleSnapshotId::from(normalize_ref_id(next));
                                        let exists = self
                                            .module_repo
                                            .load_module_variant(&mr.collection_id, &next_variant)
                                            .await
                                            .map_err(|e| {
                                                ResolveError::NotFound(format!(
                                                    "module variant load failed during replace_ref: {e}"
                                                ))
                                            })?
                                            .is_some();
                                        if !exists {
                                            return Err(ResolveError::InvalidReference(format!(
                                                "replace_ref target module variant not found: module={} variant={} path={}",
                                                mr.collection_id,
                                                next_variant,
                                                ov.path.as_str()
                                            )));
                                        }
                                        mr.variant_id = Some(next_variant);
                                    }
                                    NodeOverrideOp::Enable(false)
                                    | NodeOverrideOp::Bypass(true) => {
                                        disabled_module_ids.insert(mr.collection_id.to_string());
                                    }
                                    _ => {}
                                }
                            }
                            continue;
                        }

                        if let Some(seg_block) = segment_block(&ov.path) {
                            if let Some(br) = block_refs
                                .iter_mut()
                                .find(|br| id_matches(br.collection_id.as_str(), seg_block))
                            {
                                match &ov.op {
                                    NodeOverrideOp::ReplaceRef(next) => {
                                        let next_variant = SnapshotId::from(normalize_ref_id(next));
                                        // Validate that the replacement exists for this block collection.
                                        self.resolve_standalone_block_ref(
                                            &br.collection_id,
                                            Some(&next_variant),
                                            br.collection_id.to_string(),
                                            br.collection_id.to_string(),
                                        )
                                        .await?;
                                        br.variant_id = Some(next_variant);
                                    }
                                    NodeOverrideOp::Enable(false)
                                    | NodeOverrideOp::Bypass(true) => {
                                        disabled_block_ids.insert(br.collection_id.to_string());
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }

                    let mut modules = Vec::new();
                    for mr in &module_refs {
                        if disabled_module_ids.contains(&mr.collection_id.to_string()) {
                            continue;
                        }
                        match self
                            .resolve_module_ref(&mr.collection_id, mr.variant_id.as_ref())
                            .await
                        {
                            Ok(module) => modules.push(module),
                            Err(ResolveError::InvalidReference(_)) => {
                                // Keep existing seed/runtime behavior: unresolved base refs are skipped.
                                // ReplaceRef targets are still fail-fast validated above.
                            }
                            Err(e) => return Err(e),
                        }
                    }

                    let mut standalone_blocks = Vec::new();
                    for br in &block_refs {
                        if disabled_block_ids.contains(&br.collection_id.to_string()) {
                            continue;
                        }
                        match self
                            .resolve_standalone_block_ref(
                                &br.collection_id,
                                br.variant_id.as_ref(),
                                br.collection_id.to_string(),
                                br.collection_id.to_string(),
                            )
                            .await
                        {
                            Ok(block) => standalone_blocks.push(block),
                            Err(ResolveError::InvalidReference(_)) => {
                                // Keep existing seed/runtime behavior: unresolved base refs are skipped.
                                // ReplaceRef targets are still fail-fast validated above.
                            }
                            Err(e) => return Err(e),
                        }
                    }

                    resolved.push(ResolvedLayer {
                        layer_id: layer.id,
                        layer_variant_id: variant.id,
                        source,
                        modules,
                        standalone_blocks,
                    });
                }
            }
        }

        Ok(resolved)
    }

    /// Resolve a PatchTarget to a (rig_id, scene_id, overrides) triple.
    async fn resolve_patch_target(
        &self,
        target: &PatchTarget,
        overrides: Vec<signal_proto::overrides::Override>,
    ) -> Result<(RigId, RigSceneId, Vec<signal_proto::overrides::Override>), ResolveError> {
        let mut visited = HashSet::new();
        self.resolve_patch_target_inner(target, overrides, &mut visited)
            .await
    }

    /// Inner implementation with cycle detection via visited set.
    async fn resolve_patch_target_inner(
        &self,
        target: &PatchTarget,
        overrides: Vec<signal_proto::overrides::Override>,
        visited: &mut HashSet<String>,
    ) -> Result<(RigId, RigSceneId, Vec<signal_proto::overrides::Override>), ResolveError> {
        match target {
            PatchTarget::RigScene { rig_id, scene_id } => {
                Ok((rig_id.clone(), scene_id.clone(), overrides))
            }
            PatchTarget::Patch { patch_id } => {
                let key = patch_id.to_string();
                if !visited.insert(key) {
                    return Err(ResolveError::CycleDetected(format!(
                        "patch reference cycle at {patch_id}"
                    )));
                }
                // Find the referenced patch across all profiles
                let profiles =
                    self.profile_repo.list_profiles().await.map_err(|e| {
                        ResolveError::NotFound(format!("profiles load failed: {e}"))
                    })?;
                let referenced = profiles
                    .iter()
                    .find_map(|p| p.patch(patch_id))
                    .cloned()
                    .ok_or_else(|| {
                        ResolveError::NotFound(format!(
                            "patch cross-reference not found: {patch_id}"
                        ))
                    })?;
                // Merge overrides: referenced patch's overrides first, then ours on top
                let mut merged = referenced.overrides.clone();
                merged.extend(overrides);
                Box::pin(self.resolve_patch_target_inner(&referenced.target, merged, visited)).await
            }
            _ => Err(ResolveError::InvalidReference(format!(
                "sub-rig patch targets ({:?}) not yet resolvable to rig scene",
                target
            ))),
        }
    }

    async fn resolve_target_to_rig_scene(
        &self,
        target: &ResolveTarget,
    ) -> Result<(RigId, RigSceneId, Vec<signal_proto::overrides::Override>), ResolveError> {
        match target {
            ResolveTarget::RigScene { rig_id, scene_id } => {
                let rig = self
                    .rig_repo
                    .load_rig(rig_id)
                    .await
                    .map_err(|e| ResolveError::NotFound(format!("rig load failed: {e}")))?
                    .ok_or_else(|| ResolveError::NotFound(format!("rig not found: {rig_id}")))?;
                let scene = rig.variant(scene_id).cloned().ok_or_else(|| {
                    ResolveError::NotFound(format!("rig scene not found: {scene_id}"))
                })?;
                validate_overrides::<ScenePolicy>(&scene.overrides)
                    .map_err(|e| map_policy_err("rig scene", e))?;
                Ok((rig.id.clone(), scene.id.clone(), scene.overrides.clone()))
            }
            ResolveTarget::ProfilePatch {
                profile_id,
                patch_id,
            } => {
                let profile = self
                    .profile_repo
                    .load_profile(profile_id)
                    .await
                    .map_err(|e| ResolveError::NotFound(format!("profile load failed: {e}")))?
                    .ok_or_else(|| {
                        ResolveError::NotFound(format!("profile not found: {profile_id}"))
                    })?;
                let patch = profile.patch(patch_id).cloned().ok_or_else(|| {
                    ResolveError::NotFound(format!("patch not found: {patch_id}"))
                })?;
                validate_overrides::<FreePolicy>(&patch.overrides)
                    .map_err(|e| map_policy_err("profile patch", e))?;
                self.resolve_patch_target(&patch.target, patch.overrides)
                    .await
            }
            ResolveTarget::SongSection {
                song_id,
                section_id,
            } => {
                let song = self
                    .song_repo
                    .load_song(song_id)
                    .await
                    .map_err(|e| ResolveError::NotFound(format!("song load failed: {e}")))?
                    .ok_or_else(|| ResolveError::NotFound(format!("song not found: {song_id}")))?;
                let section = song.section(section_id).cloned().ok_or_else(|| {
                    ResolveError::NotFound(format!("section not found: {section_id}"))
                })?;
                validate_overrides::<FreePolicy>(&section.overrides)
                    .map_err(|e| map_policy_err("song section", e))?;
                match section.source {
                    signal_proto::song::SectionSource::RigScene { rig_id, scene_id } => {
                        Ok((rig_id, scene_id, section.overrides))
                    }
                    signal_proto::song::SectionSource::Patch { patch_id } => {
                        let profiles = self.profile_repo.list_profiles().await.map_err(|e| {
                            ResolveError::NotFound(format!("profiles load failed: {e}"))
                        })?;
                        let patch = profiles
                            .iter()
                            .find_map(|p| p.patch(&patch_id))
                            .cloned()
                            .ok_or_else(|| {
                                ResolveError::NotFound(format!(
                                    "section patch source not found: {patch_id}"
                                ))
                            })?;
                        validate_overrides::<FreePolicy>(&patch.overrides)
                            .map_err(|e| map_policy_err("section source patch", e))?;
                        let mut ovs = patch.overrides.clone();
                        ovs.extend(section.overrides);
                        // Use cycle-safe inner resolver with fresh visited set
                        let mut visited = HashSet::new();
                        self.resolve_patch_target_inner(&patch.target, ovs, &mut visited)
                            .await
                    }
                }
            }
        }
    }

    /// Extract the underlying `PatchTarget` from any `ResolveTarget`, following
    /// patch cross-references. Returns `None` for `RigScene` targets (which don't
    /// go through a patch).
    async fn extract_patch_target(
        &self,
        target: &ResolveTarget,
    ) -> Result<Option<PatchTarget>, ResolveError> {
        match target {
            ResolveTarget::RigScene { .. } => Ok(None),
            ResolveTarget::ProfilePatch {
                profile_id,
                patch_id,
            } => {
                let profile = self
                    .profile_repo
                    .load_profile(profile_id)
                    .await
                    .map_err(|e| ResolveError::NotFound(format!("profile load: {e}")))?
                    .ok_or_else(|| ResolveError::NotFound(format!("profile: {profile_id}")))?;
                let patch = profile
                    .patch(patch_id)
                    .cloned()
                    .ok_or_else(|| ResolveError::NotFound(format!("patch: {patch_id}")))?;
                Ok(Some(self.follow_patch_refs(patch.target).await?))
            }
            ResolveTarget::SongSection {
                song_id,
                section_id,
            } => {
                let song = self
                    .song_repo
                    .load_song(song_id)
                    .await
                    .map_err(|e| ResolveError::NotFound(format!("song load: {e}")))?
                    .ok_or_else(|| ResolveError::NotFound(format!("song: {song_id}")))?;
                let section = song
                    .section(section_id)
                    .cloned()
                    .ok_or_else(|| ResolveError::NotFound(format!("section: {section_id}")))?;
                match section.source {
                    signal_proto::song::SectionSource::RigScene { rig_id, scene_id } => {
                        Ok(Some(PatchTarget::RigScene { rig_id, scene_id }))
                    }
                    signal_proto::song::SectionSource::Patch { patch_id } => {
                        let profiles =
                            self.profile_repo.list_profiles().await.map_err(|e| {
                                ResolveError::NotFound(format!("profiles load: {e}"))
                            })?;
                        let patch = profiles
                            .iter()
                            .find_map(|p| p.patch(&patch_id))
                            .cloned()
                            .ok_or_else(|| {
                                ResolveError::NotFound(format!(
                                    "section patch source not found: {patch_id}"
                                ))
                            })?;
                        Ok(Some(self.follow_patch_refs(patch.target).await?))
                    }
                }
            }
        }
    }

    /// Follow `PatchTarget::Patch` cross-references to the terminal target.
    async fn follow_patch_refs(
        &self,
        mut target: PatchTarget,
    ) -> Result<PatchTarget, ResolveError> {
        let mut visited = HashSet::new();
        loop {
            match &target {
                PatchTarget::Patch { patch_id } => {
                    let key = patch_id.to_string();
                    if !visited.insert(key) {
                        return Err(ResolveError::CycleDetected(format!(
                            "patch reference cycle at {patch_id}"
                        )));
                    }
                    let profiles = self
                        .profile_repo
                        .list_profiles()
                        .await
                        .map_err(|e| ResolveError::NotFound(format!("profiles load: {e}")))?;
                    let patch = profiles
                        .iter()
                        .find_map(|p| p.patch(patch_id))
                        .cloned()
                        .ok_or_else(|| {
                            ResolveError::NotFound(format!(
                                "patch cross-reference not found: {patch_id}"
                            ))
                        })?;
                    target = patch.target;
                }
                _ => return Ok(target),
            }
        }
    }

    /// If the target resolves to a `PatchTarget::BlockSnapshot`, load the snapshot
    /// directly and return a minimal `ResolvedGraph` containing just that block.
    /// This IS the rig resolution for block-snapshot patches — no overrides, no megarig.
    async fn try_resolve_block_snapshot(
        &self,
        target: &ResolveTarget,
    ) -> Result<Option<ResolvedGraph>, ResolveError> {
        let patch_target = match self.extract_patch_target(target).await? {
            Some(t) => t,
            None => return Ok(None),
        };

        let PatchTarget::BlockSnapshot {
            preset_id,
            snapshot_id,
        } = &patch_target
        else {
            return Ok(None);
        };

        let snapshot = self.find_block_snapshot(preset_id, snapshot_id).await?;

        let resolved_block = ResolvedBlock {
            node_id: "plugin".to_string(),
            label: snapshot.name().to_string(),
            block_type: BlockType::Custom,
            source_preset_id: Some(preset_id.clone()),
            source_variant_id: Some(snapshot_id.clone()),
            state_data: snapshot.state_data().map(|d| d.to_vec()),
            block: snapshot.block(),
            stale: false,
        };

        let graph = ResolvedGraph {
            target: target.clone(),
            rig_id: RigId::from(signal_proto::seed_id("block-snapshot-rig")),
            rig_scene_id: RigSceneId::from(signal_proto::seed_id("block-snapshot-scene")),
            engines: vec![ResolvedEngine {
                engine_id: EngineId::from(signal_proto::seed_id("block-snapshot-engine")),
                engine_scene_id: EngineSceneId::from(signal_proto::seed_id(
                    "block-snapshot-engine-scene",
                )),
                layers: vec![ResolvedLayer {
                    layer_id: LayerId::from(signal_proto::seed_id("block-snapshot-layer")),
                    layer_variant_id: LayerSnapshotId::from(signal_proto::seed_id(
                        "block-snapshot-layer-variant",
                    )),
                    source: LayerSource::InlinedInParent,
                    modules: vec![ResolvedModule {
                        source_preset_id: ModulePresetId::from(signal_proto::seed_id(
                            "block-snapshot-module",
                        )),
                        source_variant_id: ModuleSnapshotId::from(signal_proto::seed_id(
                            "block-snapshot-module-variant",
                        )),
                        blocks: vec![resolved_block],
                    }],
                    standalone_blocks: vec![],
                }],
            }],
            effective_overrides: vec![],
        };

        Ok(Some(graph))
    }

    /// Search for a block snapshot across all block types (Custom first).
    async fn find_block_snapshot(
        &self,
        preset_id: &PresetId,
        snapshot_id: &SnapshotId,
    ) -> Result<Snapshot, ResolveError> {
        for block_type in std::iter::once(BlockType::Custom).chain(ALL_BLOCK_TYPES.iter().copied())
        {
            if let Ok(Some(snap)) = self
                .block_repo
                .load_block_variant(block_type, preset_id, snapshot_id)
                .await
            {
                return Ok(snap);
            }
        }
        Err(ResolveError::NotFound(format!(
            "block snapshot not found: preset={preset_id}, snapshot={snapshot_id}"
        )))
    }
}

impl<B, M, L, E, R, P, So, Se, St, Ra> ResolveService
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
    async fn resolve_target(&self, target: ResolveTarget) -> Result<ResolvedGraph, ResolveError> {
        // BlockSnapshot targets resolve directly — the snapshot IS the rig.
        if let Some(graph) = self.try_resolve_block_snapshot(&target).await? {
            return Ok(graph);
        }

        let (rig_id, rig_scene_id, higher_scope_overrides) =
            self.resolve_target_to_rig_scene(&target).await?;

        let rig = self
            .rig_repo
            .load_rig(&rig_id)
            .await
            .map_err(|e| ResolveError::NotFound(format!("rig load failed: {e}")))?
            .ok_or_else(|| ResolveError::NotFound(format!("rig not found: {rig_id}")))?;
        let rig_scene = rig.variant(&rig_scene_id).cloned().ok_or_else(|| {
            ResolveError::NotFound(format!("rig scene not found: {rig_scene_id}"))
        })?;
        validate_overrides::<ScenePolicy>(&rig_scene.overrides)
            .map_err(|e| map_policy_err("rig scene", e))?;

        let mut engines = Vec::new();
        let global_overrides =
            merge_override_levels(&[rig_scene.overrides.clone(), higher_scope_overrides.clone()]);
        let mut level_overrides = vec![global_overrides.clone()];

        for engine_sel in &rig_scene.engine_selections {
            let mut selected_engine_scene_id = engine_sel.variant_id.clone();
            let mut engine_enabled = true;
            let mut engine_replace_path: Option<String> = None;
            for ov in &global_overrides {
                let Some(seg_engine) = segment_engine(&ov.path) else {
                    continue;
                };
                if !id_matches(engine_sel.engine_id.as_str(), seg_engine) {
                    continue;
                }
                if segment_layer(&ov.path).is_none()
                    && segment_module(&ov.path).is_none()
                    && segment_block(&ov.path).is_none()
                    && segment_param(&ov.path).is_none()
                {
                    match &ov.op {
                        NodeOverrideOp::ReplaceRef(next) => {
                            selected_engine_scene_id = EngineSceneId::from(normalize_ref_id(next));
                            engine_replace_path = Some(ov.path.as_str());
                        }
                        NodeOverrideOp::Enable(false) | NodeOverrideOp::Bypass(true) => {
                            engine_enabled = false;
                        }
                        _ => {}
                    }
                }
            }
            if !engine_enabled {
                continue;
            }

            let engine = self
                .engine_repo
                .load_engine(&engine_sel.engine_id)
                .await
                .map_err(|e| ResolveError::NotFound(format!("engine load failed: {e}")))?
                .ok_or_else(|| {
                    ResolveError::InvalidReference(format!(
                        "missing engine ref: {}",
                        engine_sel.engine_id
                    ))
                })?;
            if engine_replace_path.is_some()
                && !engine
                    .variants
                    .iter()
                    .any(|v| v.id == selected_engine_scene_id)
            {
                return Err(ResolveError::InvalidReference(format!(
                    "replace_ref target engine scene not found: engine={} variant={} path={}",
                    engine.id,
                    selected_engine_scene_id,
                    engine_replace_path.unwrap_or_default()
                )));
            }
            let engine_scene = engine
                .variant(&selected_engine_scene_id)
                .cloned()
                .ok_or_else(|| {
                    ResolveError::InvalidReference(format!(
                        "missing engine scene ref: {}::{}",
                        engine_sel.engine_id, selected_engine_scene_id
                    ))
                })?;
            validate_overrides::<ScenePolicy>(&engine_scene.overrides)
                .map_err(|e| map_policy_err("engine scene", e))?;
            let engine_scope_overrides =
                merge_override_levels(&[global_overrides.clone(), engine_scene.overrides.clone()]);
            level_overrides.push(engine_scope_overrides.clone());

            let mut resolved_layers = Vec::new();
            for layer_sel in &engine_scene.layer_selections {
                let mut selected_layer_variant_id = layer_sel.variant_id.clone();
                let mut layer_enabled = true;
                let mut layer_replace_path: Option<String> = None;
                for ov in &engine_scope_overrides {
                    let Some(seg_layer) = segment_layer(&ov.path) else {
                        continue;
                    };
                    if !id_matches(layer_sel.layer_id.as_str(), seg_layer) {
                        continue;
                    }
                    if let Some(seg_engine) = segment_engine(&ov.path) {
                        if !id_matches(engine.id.as_str(), seg_engine) {
                            continue;
                        }
                    }
                    if segment_module(&ov.path).is_none()
                        && segment_block(&ov.path).is_none()
                        && segment_param(&ov.path).is_none()
                    {
                        match &ov.op {
                            NodeOverrideOp::ReplaceRef(next) => {
                                selected_layer_variant_id =
                                    LayerSnapshotId::from(normalize_ref_id(next));
                                layer_replace_path = Some(ov.path.as_str());
                            }
                            NodeOverrideOp::Enable(false) | NodeOverrideOp::Bypass(true) => {
                                layer_enabled = false;
                            }
                            _ => {}
                        }
                    }
                }
                if !layer_enabled {
                    continue;
                }
                let selected_layer_variant = self
                    .layer_repo
                    .load_variant(&layer_sel.layer_id, &selected_layer_variant_id)
                    .await
                    .map_err(|e| {
                        ResolveError::NotFound(format!("layer variant load failed: {e}"))
                    })?;
                if selected_layer_variant.is_none() {
                    if let Some(path) = layer_replace_path {
                        return Err(ResolveError::InvalidReference(format!(
                            "replace_ref target layer variant not found: layer={} variant={} path={}",
                            layer_sel.layer_id, selected_layer_variant_id, path
                        )));
                    }
                    return Err(ResolveError::InvalidReference(format!(
                        "missing layer variant ref: layer={} variant={}",
                        layer_sel.layer_id, selected_layer_variant_id
                    )));
                }

                let mut layers = self
                    .resolve_layer_tree(
                        &engine.id,
                        layer_sel.layer_id.clone(),
                        selected_layer_variant_id.clone(),
                        LayerSource::LayerPreset {
                            layer_id: layer_sel.layer_id.clone(),
                            variant_id: selected_layer_variant_id.clone(),
                        },
                        &engine_scope_overrides,
                    )
                    .await?;
                resolved_layers.append(&mut layers);

                if let Some(layer) = selected_layer_variant {
                    level_overrides.push(layer.overrides.clone());
                }
            }

            engines.push(ResolvedEngine {
                engine_id: engine.id.clone(),
                engine_scene_id: engine_scene.id.clone(),
                layers: resolved_layers,
            });
        }

        let mut graph = ResolvedGraph {
            target,
            rig_id,
            rig_scene_id,
            engines,
            effective_overrides: merge_override_levels(&level_overrides),
        };
        apply_effective_set_overrides(&mut graph);
        Ok(graph)
    }
}
