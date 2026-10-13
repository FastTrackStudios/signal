//! `impl Projects for Standalone` — stub. Per-project handle pattern
//! retired with the architect::rpc port; all methods take
//! `ProjectContext` now. Full impl pending — currently todo!() to keep
//! the workspace compiling.

use daw_proto::{DawResult, ProjectContext, ProjectInfo, Projects, UndoScope};

use super::daw::Standalone;

/// Thin per-project handle for `Daw::current_project()` / `project()`.
/// Post-port `Projects` methods all take `ProjectContext` so this
/// wrapper just forwards to `Standalone`.
pub struct StandaloneProject<'a> {
    daw: &'a Standalone,
    #[allow(dead_code)]
    guid: String,
}

impl<'a> StandaloneProject<'a> {
    pub(crate) fn new(daw: &'a Standalone, guid: String) -> Self {
        Self { daw, guid }
    }
}

impl<'a> Projects for StandaloneProject<'a> {
    fn info(&self, project: ProjectContext) -> DawResult<ProjectInfo> {
        self.daw.info(project)
    }
    fn current(&self) -> Option<ProjectInfo> {
        self.daw.current()
    }
    fn get(&self, id: &str) -> Option<ProjectInfo> {
        self.daw.get(id)
    }
    fn list(&self) -> Vec<ProjectInfo> {
        self.daw.list()
    }
    fn get_by_slot(&self, slot: u32) -> Option<ProjectInfo> {
        self.daw.get_by_slot(slot)
    }
    fn select(&self, id: &str) -> bool {
        self.daw.select(id)
    }
    fn open(&self, path: &str) -> Option<ProjectInfo> {
        self.daw.open(path)
    }
    fn create(&self) -> Option<ProjectInfo> {
        self.daw.create()
    }
    fn close(&self, id: &str) -> bool {
        self.daw.close(id)
    }
    fn begin_undo_block(&self, project: ProjectContext, label: &str) {
        self.daw.begin_undo_block(project, label)
    }
    fn end_undo_block(&self, project: ProjectContext, label: &str, scope: Option<UndoScope>) {
        self.daw.end_undo_block(project, label, scope)
    }
    fn undo(&self, project: ProjectContext) -> bool {
        self.daw.undo(project)
    }
    fn redo(&self, project: ProjectContext) -> bool {
        self.daw.redo(project)
    }
    fn last_undo_label(&self, project: ProjectContext) -> Option<String> {
        self.daw.last_undo_label(project)
    }
    fn last_redo_label(&self, project: ProjectContext) -> Option<String> {
        self.daw.last_redo_label(project)
    }
    fn run_command(&self, project: ProjectContext, command: &str) -> bool {
        self.daw.run_command(project, command)
    }
    fn save(&self, project: ProjectContext) {
        self.daw.save(project)
    }
    fn save_all(&self) {
        self.daw.save_all()
    }
    fn get_project_info_string(&self, project: ProjectContext, key: &str) -> String {
        self.daw.get_project_info_string(project, key)
    }
    fn set_project_info_string(&self, project: ProjectContext, key: &str, value: &str) {
        self.daw.set_project_info_string(project, key, value)
    }
    fn get_project_info(&self, project: ProjectContext, key: &str) -> f64 {
        self.daw.get_project_info(project, key)
    }
    fn set_project_info(&self, project: ProjectContext, key: &str, value: f64) {
        self.daw.set_project_info(project, key, value)
    }
    fn get_project_config(&self, project: ProjectContext, key: &str) -> Option<f64> {
        self.daw.get_project_config(project, key)
    }
    fn set_project_config(&self, project: ProjectContext, key: &str, value: f64) -> bool {
        self.daw.set_project_config(project, key, value)
    }
    fn set_ruler_lane_name(&self, project: ProjectContext, lane_index: u32, name: &str) {
        self.daw.set_ruler_lane_name(project, lane_index, name)
    }
    fn get_ruler_lane_name(&self, project: ProjectContext, lane_index: u32) -> String {
        self.daw.get_ruler_lane_name(project, lane_index)
    }
    fn ruler_lane_count(&self, project: ProjectContext) -> u32 {
        self.daw.ruler_lane_count(project)
    }
}

/// Resolve a `ProjectContext::Current` to the live current-project
/// guid, or pass through an explicit `Project(guid)`. `None` if no
/// project is current and the context was `Current`.
fn resolve_ctx_guid(daw: &Standalone, ctx: &ProjectContext) -> Option<String> {
    match ctx {
        ProjectContext::Project(g) => Some(g.clone()),
        ProjectContext::Current => daw.state.lock().ok()?.current_project_guid.clone(),
    }
}

impl Projects for Standalone {
    fn info(&self, project: ProjectContext) -> DawResult<ProjectInfo> {
        let guid = resolve_ctx_guid(self, &project)
            .ok_or_else(|| daw_proto::DawError::not_found("project", "current"))?;
        let state = self
            .state
            .lock()
            .map_err(|_| daw_proto::DawError::internal("state poisoned"))?;
        state
            .projects
            .get(&guid)
            .map(|p| p.info.clone())
            .ok_or_else(|| daw_proto::DawError::not_found("project", &guid))
    }

    fn current(&self) -> Option<ProjectInfo> {
        let s = self.state.lock().ok()?;
        let guid = s.current_project_guid.as_ref()?;
        s.projects.get(guid).map(|p| p.info.clone())
    }

    fn get(&self, project_id: &str) -> Option<ProjectInfo> {
        let s = self.state.lock().ok()?;
        s.projects.get(project_id).map(|p| p.info.clone())
    }

    fn list(&self) -> Vec<ProjectInfo> {
        let Ok(s) = self.state.lock() else {
            return Vec::new();
        };
        let mut out: Vec<ProjectInfo> = s.projects.values().map(|p| p.info.clone()).collect();
        // Deterministic ordering by name so tests can rely on it.
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    fn get_by_slot(&self, slot: u32) -> Option<ProjectInfo> {
        self.list().into_iter().nth(slot as usize)
    }

    fn select(&self, project_id: &str) -> bool {
        let Ok(mut s) = self.state.lock() else {
            return false;
        };
        if s.projects.contains_key(project_id) {
            s.current_project_guid = Some(project_id.to_string());
            true
        } else {
            false
        }
    }

    /// Load a `.rpp` file from disk into a new project tab. Native-only
    /// — wasm callers should use `crate::project_loader::load_rpp_text`
    /// directly with a `BayFileResolver`.
    fn open(&self, path: &str) -> Option<ProjectInfo> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let text = std::fs::read_to_string(path).ok()?;
            let name = std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("project")
                .to_string();
            #[cfg(any(feature = "rpp-project", feature = "rpp-project-wasm"))]
            {
                let summary =
                    crate::project_loader::load_rpp_text(self, &name, path, &text).ok()?;
                // r[impl drums.open.rpp]
                // Media resolves against the file's directory, as REAPER
                // does; a source that cannot be found is a per-take entry
                // in the materialize report, never a failed open.
                #[cfg(feature = "decode")]
                {
                    let dir = std::path::Path::new(path)
                        .parent()
                        .map(|d| d.to_path_buf())
                        .unwrap_or_default();
                    self.media_bay().set_file_resolver(Box::new(
                        crate::media_bay::ProjectRelativeResolver::new(dir),
                    ));
                    let _ = crate::audio_engine::materialize::materialize_via_bay(
                        self,
                        &summary.project_guid,
                    );
                }
                let s = self.state.lock().ok()?;
                s.projects
                    .get(&summary.project_guid)
                    .map(|p| p.info.clone())
            }
            #[cfg(not(any(feature = "rpp-project", feature = "rpp-project-wasm")))]
            {
                let _ = text;
                let _ = name;
                None
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            None
        }
    }

    /// Create a new empty project tab + make it current. Synthesises
    /// a fresh GUID + a default name.
    fn create(&self) -> Option<ProjectInfo> {
        let guid = uuid::Uuid::new_v4().to_string();
        let info = ProjectInfo {
            guid: guid.clone(),
            name: format!("Untitled-{}", &guid[..8]),
            path: String::new(),
        };
        self.seed_project(info.clone());
        Some(info)
    }

    fn close(&self, project_id: &str) -> bool {
        let Ok(mut s) = self.state.lock() else {
            return false;
        };
        if s.projects.remove(project_id).is_some() {
            if s.current_project_guid.as_deref() == Some(project_id) {
                // Fall back to any remaining project, else None.
                s.current_project_guid = s.projects.keys().next().cloned();
            }
            true
        } else {
            false
        }
    }

    /// Snapshot-based undo over the project's edit surface.
    ///
    /// `begin` captures what the edit is about to change; `end` commits
    /// that capture as one undo step under its label. Everything a block
    /// does — a whole group split across seven mics — is then one step,
    /// which is the semantics REAPER's block gives and the reason the
    /// facade has the begin/end ceremony at all.
    // r[impl drums.quantize.undo]
    fn begin_undo_block(&self, project: ProjectContext, label: &str) {
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return;
        };
        let Ok(mut s) = self.state.lock() else {
            return;
        };
        let Some(p) = s.projects.get(&guid) else {
            return;
        };
        let step = crate::sync::daw::UndoStep {
            label: label.to_string(),
            snapshot: crate::sync::daw::EditSnapshot::capture(p),
        };
        // A begin with no end is abandoned, not stacked.
        s.pending_undo.insert(guid, step);
    }

    fn end_undo_block(&self, project: ProjectContext, label: &str, _scope: Option<UndoScope>) {
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return;
        };
        let Ok(mut s) = self.state.lock() else {
            return;
        };
        let Some(mut step) = s.pending_undo.remove(&guid) else {
            return;
        };
        if !label.is_empty() {
            step.label = label.to_string();
        }
        let named = step.label.clone();
        let stack = s.undo.entry(guid.clone()).or_default();
        stack.push(step);
        if stack.len() > crate::sync::daw::UNDO_LIMIT {
            stack.remove(0);
        }
        // A new edit invalidates the redo branch, as everywhere.
        s.redo.remove(&guid);
        drop(s);
        crate::steps::finished(&guid, &named);
    }

    // r[impl drums.quantize.undo]
    fn undo(&self, project: ProjectContext) -> bool {
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return false;
        };
        let Ok(mut s) = self.state.lock() else {
            return false;
        };
        let Some(step) = s.undo.get_mut(&guid).and_then(Vec::pop) else {
            return false;
        };
        let Some(p) = s.projects.get_mut(&guid) else {
            return false;
        };
        let redo_step = crate::sync::daw::UndoStep {
            label: step.label.clone(),
            snapshot: crate::sync::daw::EditSnapshot::capture(p),
        };
        step.snapshot.restore(p);
        s.redo.entry(guid.clone()).or_default().push(redo_step);
        drop(s);
        crate::steps::finished(&guid, &format!("Undo {}", step.label));
        true
    }

    fn redo(&self, project: ProjectContext) -> bool {
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return false;
        };
        let Ok(mut s) = self.state.lock() else {
            return false;
        };
        let Some(step) = s.redo.get_mut(&guid).and_then(Vec::pop) else {
            return false;
        };
        let Some(p) = s.projects.get_mut(&guid) else {
            return false;
        };
        let undo_step = crate::sync::daw::UndoStep {
            label: step.label.clone(),
            snapshot: crate::sync::daw::EditSnapshot::capture(p),
        };
        step.snapshot.restore(p);
        s.undo.entry(guid.clone()).or_default().push(undo_step);
        drop(s);
        crate::steps::finished(&guid, &format!("Redo {}", step.label));
        true
    }

    fn last_undo_label(&self, project: ProjectContext) -> Option<String> {
        let guid = resolve_ctx_guid(self, &project)?;
        let s = self.state.lock().ok()?;
        s.undo.get(&guid)?.last().map(|u| u.label.clone())
    }

    fn last_redo_label(&self, project: ProjectContext) -> Option<String> {
        let guid = resolve_ctx_guid(self, &project)?;
        let s = self.state.lock().ok()?;
        s.redo.get(&guid)?.last().map(|u| u.label.clone())
    }
    fn run_command(&self, _project: ProjectContext, _command: &str) -> bool {
        // No REAPER action registry on standalone; commands no-op
        // and return false (not found). Custom actions can be
        // routed through the `action_registry` service if needed.
        false
    }

    /// With `rpp-save` on, saving writes a **new** `.rpp` beside the
    /// original — never over it — and remembers where it went (see
    /// [`crate::save::save_project_as`], `last_saved_path`). Without
    /// the feature the call is accepted and does nothing, keeping the
    /// trait surface usable.
    // r[impl drums.save.new-file]
    fn save(&self, project: ProjectContext) {
        #[cfg(feature = "rpp-save")]
        {
            let Some(guid) = resolve_ctx_guid(self, &project) else {
                return;
            };
            match crate::save::save_project_as(self, &guid) {
                Ok(path) => {
                    let written = path.to_string_lossy().into_owned();
                    let _ = self.with_project_mut(&guid, |p| {
                        p.project_ext_state
                            .insert((String::new(), "LAST_SAVED_PATH".into()), written.clone());
                    });
                    tracing::info!(path = %written, "saved project as a new file");
                }
                Err(e) => tracing::warn!(error = %e, "save failed"),
            }
        }
        #[cfg(not(feature = "rpp-save"))]
        let _ = project;
    }
    fn save_all(&self) {}

    fn get_project_info_string(&self, project: ProjectContext, key: &str) -> String {
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return String::new();
        };
        let Ok(s) = self.state.lock() else {
            return String::new();
        };
        let Some(p) = s.projects.get(&guid) else {
            return String::new();
        };
        // Map a handful of canonical REAPER `GetSetProjectInfo_String`
        // keys to fields we model. Unknown keys return empty.
        match key {
            "PROJECT_NAME" => p.info.name.clone(),
            "PROJECT_PATH" => p.info.path.clone(),
            "PROJECT_NOTES" => p
                .project_ext_state
                .get(&(String::new(), "PROJECT_NOTES".into()))
                .cloned()
                .unwrap_or_default(),
            _ => p
                .project_ext_state
                .get(&(String::new(), key.to_string()))
                .cloned()
                .unwrap_or_default(),
        }
    }

    fn set_project_info_string(&self, project: ProjectContext, key: &str, value: &str) {
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return;
        };
        let _ = self.with_project_mut(&guid, |p| match key {
            "PROJECT_NAME" => p.info.name = value.to_string(),
            "PROJECT_PATH" => p.info.path = value.to_string(),
            _ => {
                p.project_ext_state
                    .insert((String::new(), key.to_string()), value.to_string());
            }
        });
    }

    fn get_project_info(&self, project: ProjectContext, key: &str) -> f64 {
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return 0.0;
        };
        let Ok(s) = self.state.lock() else { return 0.0 };
        let Some(p) = s.projects.get(&guid) else {
            return 0.0;
        };
        // Surface the most-used REAPER numeric keys. Unknown keys = 0.
        match key {
            "PROJECT_TIMESIG_NUM" => p.transport.time_signature.numerator as f64,
            "PROJECT_TIMESIG_DENOM" => p.transport.time_signature.denominator as f64,
            "PROJECT_BPM" => p.transport.tempo.bpm(),
            _ => ruler_lane_flags_key(key)
                .and_then(|i| p.ruler_lanes.get(&i))
                .map_or(0.0, |lane| f64::from(lane.flags)),
        }
    }

    fn set_project_info(&self, project: ProjectContext, key: &str, value: f64) {
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return;
        };
        let _ = self.with_project_mut(&guid, |p| match key {
            "PROJECT_BPM" => {
                p.transport.tempo = daw_proto::primitives::Tempo::from_bpm(value.max(1.0));
            }
            "PROJECT_TIMESIG_NUM" => {
                let denom = p.transport.time_signature.denominator;
                p.transport.time_signature =
                    daw_proto::primitives::TimeSignature::new(value as u32, denom);
            }
            "PROJECT_TIMESIG_DENOM" => {
                let num = p.transport.time_signature.numerator;
                p.transport.time_signature =
                    daw_proto::primitives::TimeSignature::new(num, value as u32);
            }
            other => {
                if let Some(i) = ruler_lane_flags_key(other) {
                    set_ruler_lane_flags(&mut p.ruler_lanes, i, value as u32);
                }
            }
        });
    }

    fn get_project_config(&self, project: ProjectContext, key: &str) -> Option<f64> {
        let guid = resolve_ctx_guid(self, &project)?;
        let s = self.state.lock().ok()?;
        let value = s
            .projects
            .get(&guid)?
            .project_ext_state
            .get(&("daw-standalone:project_config".into(), key.to_string()))?
            .parse()
            .ok()?;
        Some(value)
    }

    fn set_project_config(&self, project: ProjectContext, key: &str, value: f64) -> bool {
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return false;
        };
        self.with_project_mut(&guid, |p| {
            p.project_ext_state.insert(
                ("daw-standalone:project_config".into(), key.to_string()),
                value.to_string(),
            );
        })
        .is_ok()
    }

    fn set_ruler_lane_name(&self, project: ProjectContext, lane_index: u32, name: &str) {
        // Naming a lane creates it, as `RULER_LANE_NAME:N` does in REAPER.
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return;
        };
        let _ = self.with_project_mut(&guid, |p| {
            p.ruler_lanes.entry(lane_index).or_default().name = name.to_string();
        });
    }

    fn get_ruler_lane_name(&self, project: ProjectContext, lane_index: u32) -> String {
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return String::new();
        };
        let Ok(s) = self.state.lock() else {
            return String::new();
        };
        s.projects
            .get(&guid)
            .and_then(|p| p.ruler_lanes.get(&lane_index))
            .map(|lane| lane.name.clone())
            .unwrap_or_default()
    }

    fn ruler_lane_count(&self, project: ProjectContext) -> u32 {
        let Some(guid) = resolve_ctx_guid(self, &project) else {
            return 0;
        };
        let Ok(s) = self.state.lock() else { return 0 };
        // REAPER's lanes are a contiguous 0..count run; a gap is an
        // unnamed lane, not a missing one. So the count is one past the
        // highest index, never the number of named entries.
        s.projects
            .get(&guid)
            .and_then(|p| p.ruler_lanes.keys().next_back())
            .map_or(0, |last| last + 1)
    }
}

/// The lane index in a `RULER_LANE_FLAGS:N` project-info key.
fn ruler_lane_flags_key(key: &str) -> Option<u32> {
    key.strip_prefix("RULER_LANE_FLAGS:")?.parse().ok()
}

/// Set a lane's flags. A default flag is exclusive, as in REAPER: making
/// one lane the default region (or marker) lane takes it off the others.
fn set_ruler_lane_flags(
    lanes: &mut std::collections::BTreeMap<u32, crate::sync::RulerLane>,
    index: u32,
    flags: u32,
) {
    use crate::sync::RulerLane;
    for exclusive in [RulerLane::DEFAULT_REGION, RulerLane::DEFAULT_MARKER] {
        if flags & exclusive != 0 {
            for (i, lane) in lanes.iter_mut() {
                if *i != index {
                    lane.flags &= !exclusive;
                }
            }
        }
    }
    lanes.entry(index).or_default().flags = flags;
}

/// The lane a fresh region (`DEFAULT_REGION`) or marker (`DEFAULT_MARKER`)
/// lands on, if the project has flagged one.
pub(crate) fn default_lane(
    lanes: &std::collections::BTreeMap<u32, crate::sync::RulerLane>,
    flag: u32,
) -> Option<u32> {
    lanes
        .iter()
        .find(|(_, lane)| lane.flags & flag != 0)
        .map(|(i, _)| *i)
}

/// Whether a marker or region of `p` already carries `guid`. Markers and
/// regions are one list in the project file, matched by GUID on save, so
/// they share one namespace here too.
pub(crate) fn marker_or_region_guid_taken(p: &super::daw::ProjectState, guid: &str) -> bool {
    p.markers.values().any(|m| m.guid.as_deref() == Some(guid))
        || p.regions.values().any(|r| r.guid.as_deref() == Some(guid))
}
