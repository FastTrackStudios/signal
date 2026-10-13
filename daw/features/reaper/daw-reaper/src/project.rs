//! `impl Projects for Reaper` — project tabs, undo, commands, info, ruler lanes.

use daw_proto::{DawError, DawResult, ProjectInfo, Projects};
use reaper_high::{Project, Reaper};
use reaper_medium::{CommandId, ProjectContext, ProjectPart, ProjectRef, UndoScope};
use tracing::{debug, info};

use crate::project_context::{MAX_PROJECT_TABS, find_project_by_guid, project_guid};

// Thread-local storage for the undo block label.
//
// `begin_undo_block` and `end_undo_block` arrive as separate RPC calls, but
// REAPER's `Undo_EndBlock2` needs the label at end-time. We stash the label
// from `begin` and retrieve it in `end` as a fallback.
thread_local! {
    pub(crate) static UNDO_LABEL: std::cell::Cell<Option<String>> = const { std::cell::Cell::new(None) };
}

/// Resolve a daw_proto::ProjectContext to a reaper_high::Project
fn resolve_project(ctx: &daw_proto::ProjectContext) -> Option<Project> {
    match ctx {
        daw_proto::ProjectContext::Current => Some(Reaper::get().current_project()),
        daw_proto::ProjectContext::Project(guid) => find_project_by_guid(guid),
    }
}

/// Get a project by tab index using medium_reaper's enum_projects
fn project_by_tab(reaper: &Reaper, tab_index: u32) -> Option<Project> {
    reaper
        .medium_reaper()
        .enum_projects(ProjectRef::Tab(tab_index), 0)
        .map(|result| Project::new(result.project))
}

/// Extract project info from a REAPER project
pub(crate) fn project_to_info(project: &Project) -> ProjectInfo {
    let path = project.file().map(|p| p.to_string()).unwrap_or_default();
    let name = if path.is_empty() {
        "Untitled".to_string()
    } else {
        std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    };

    let guid = project_guid(project);

    ProjectInfo { guid, name, path }
}

/// Convert daw_proto::UndoScope to reaper_medium::UndoScope
pub(crate) fn convert_undo_scope(scope: &daw_proto::UndoScope) -> UndoScope {
    use enumflags2::BitFlags;

    match scope {
        daw_proto::UndoScope::All => UndoScope::All,
        daw_proto::UndoScope::Scoped(parts) => {
            let mut flags = BitFlags::empty();
            for part in parts {
                let reaper_part = match part {
                    daw_proto::ProjectPart::Freeze => ProjectPart::Freeze,
                    daw_proto::ProjectPart::Fx => ProjectPart::Fx,
                    daw_proto::ProjectPart::Items => ProjectPart::Items,
                    daw_proto::ProjectPart::MiscCfg => ProjectPart::MiscCfg,
                    daw_proto::ProjectPart::TrackCfg => ProjectPart::TrackCfg,
                };
                flags |= reaper_part;
            }
            UndoScope::Scoped(flags)
        }
    }
}

fn config_value_for_profile(key: &str, size: u32, value: f64) -> String {
    if matches!(key, "preroll" | "projmetroen" | "projmetrobeatlen") {
        return value.round().to_string();
    }

    if size == std::mem::size_of::<f64>() as u32 || size == std::mem::size_of::<f32>() as u32 {
        format!("{value:.14}")
    } else {
        value.round().to_string()
    }
}

#[cfg(target_family = "unix")]
fn persist_global_config_value(key: &str, value: &str) -> bool {
    let Ok(section) = std::ffi::CString::new("REAPER") else {
        return false;
    };
    let Ok(key) = std::ffi::CString::new(key) else {
        return false;
    };
    let Ok(value) = std::ffi::CString::new(value) else {
        return false;
    };

    let ini_path = Reaper::get()
        .medium_reaper()
        .get_ini_file(|path| path.to_path_buf());
    let Ok(ini_path) = std::ffi::CString::new(ini_path.as_str()) else {
        return false;
    };

    unsafe {
        reaper_low::Swell::get().WritePrivateProfileString(
            section.as_ptr(),
            key.as_ptr(),
            value.as_ptr(),
            ini_path.as_ptr(),
        ) != 0
    }
}

#[cfg(not(target_family = "unix"))]
fn persist_global_config_value(_key: &str, _value: &str) -> bool {
    true
}

impl Projects for crate::Reaper {
    fn info(&self, project: daw_proto::ProjectContext) -> DawResult<ProjectInfo> {
        let proj = resolve_project(&project)
            .ok_or_else(|| DawError::not_found("Project", &format!("{:?}", project)))?;
        Ok(project_to_info(&proj))
    }

    fn current(&self) -> Option<ProjectInfo> {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        Some(project_to_info(&project))
    }

    fn get(&self, project_id: &str) -> Option<ProjectInfo> {
        let reaper = Reaper::get();

        // Iterate through all open project tabs
        for i in 0..MAX_PROJECT_TABS {
            if let Some(project) = project_by_tab(reaper, i) {
                let info = project_to_info(&project);
                if info.guid == project_id {
                    return Some(info);
                }
            } else {
                // No more tabs
                break;
            }
        }
        None
    }

    fn list(&self) -> Vec<ProjectInfo> {
        let reaper = Reaper::get();
        let mut projects = Vec::new();

        // Iterate through all open project tabs (max 128)
        for i in 0..MAX_PROJECT_TABS {
            if let Some(project) = project_by_tab(reaper, i) {
                let info = project_to_info(&project);
                // Skip routing/utility projects
                if !info.name.to_uppercase().contains("FTS-ROUTING") {
                    projects.push(info);
                }
            } else {
                // No more tabs
                break;
            }
        }

        info!("Projects::list - found {} projects", projects.len());
        projects
    }

    fn get_by_slot(&self, slot: u32) -> Option<ProjectInfo> {
        let reaper = Reaper::get();
        project_by_tab(reaper, slot).map(|p| project_to_info(&p))
    }

    fn select(&self, project_id: &str) -> bool {
        info!("Projects::select({})", project_id);

        let reaper = Reaper::get();

        // Find the tab index for the project with matching GUID
        let mut target_tab: Option<u32> = None;
        for i in 0..MAX_PROJECT_TABS {
            if let Some(project) = project_by_tab(reaper, i) {
                let info = project_to_info(&project);
                if info.guid == project_id {
                    target_tab = Some(i);
                    break;
                }
            } else {
                break;
            }
        }

        let Some(target) = target_tab else {
            info!("Projects::select - project {} not found", project_id);
            return false;
        };

        // Get current tab index
        let current_project = reaper.current_project();
        let mut current_tab: Option<u32> = None;
        for i in 0..MAX_PROJECT_TABS {
            if let Some(project) = project_by_tab(reaper, i) {
                if project == current_project {
                    current_tab = Some(i);
                    break;
                }
            } else {
                break;
            }
        }

        let Some(current) = current_tab else {
            return false;
        };

        if current == target {
            // Already on the correct tab
            return true;
        }

        // Calculate shortest path (forward or backward)
        // Count total tabs first
        let mut total_tabs = 0u32;
        for i in 0..MAX_PROJECT_TABS {
            if project_by_tab(reaper, i).is_some() {
                total_tabs = i + 1;
            } else {
                break;
            }
        }

        let forward_distance = if target > current {
            target - current
        } else {
            total_tabs - current + target
        };

        let backward_distance = if current > target {
            current - target
        } else {
            current + total_tabs - target
        };

        // REAPER actions for tab switching
        let action_next_tab = CommandId::new(40861);
        let action_prev_tab = CommandId::new(40862);

        if forward_distance <= backward_distance {
            // Go forward
            for _ in 0..forward_distance {
                reaper.medium_reaper().main_on_command_ex(
                    action_next_tab,
                    0,
                    ProjectContext::CurrentProject,
                );
            }
        } else {
            // Go backward
            for _ in 0..backward_distance {
                reaper.medium_reaper().main_on_command_ex(
                    action_prev_tab,
                    0,
                    ProjectContext::CurrentProject,
                );
            }
        }

        // Verify we ended up at the right project
        let final_project = reaper.current_project();
        let final_info = project_to_info(&final_project);
        let success = final_info.guid == project_id;

        if success {
            info!(
                "Projects::select - successfully switched to {}",
                final_info.name
            );
        } else {
            tracing::warn!(
                "Projects::select - ended at {} instead of expected project",
                final_info.name
            );
        }

        success
    }

    fn open(&self, path: &str) -> Option<ProjectInfo> {
        info!("Projects::open({})", path);

        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        // Snapshot existing tab pointers before opening
        let mut existing_ptrs = std::collections::HashSet::new();
        for tab in 0..MAX_PROJECT_TABS {
            match project_by_tab(reaper, tab) {
                Some(p) => {
                    existing_ptrs.insert(p.raw().as_ptr() as usize);
                }
                None => break,
            }
        }

        // Create a new tab first (action 41929 = "New project tab, ignore default template")
        let action_new_tab = CommandId::new(41929);
        medium.main_on_command_ex(action_new_tab, 0, ProjectContext::CurrentProject);

        // Open the project file into the new tab (noprompt to skip save dialog)
        let file_path = camino::Utf8Path::new(path);
        let mut behavior = reaper_medium::OpenProjectBehavior::default();
        behavior.open_as_template = false;
        behavior.prompt = false;
        medium.main_open_project(file_path, behavior);

        // Find the new tab by scanning for a pointer not in our snapshot
        for tab in 0..MAX_PROJECT_TABS {
            if let Some(p) = project_by_tab(reaper, tab) {
                let ptr = p.raw().as_ptr() as usize;
                if !existing_ptrs.contains(&ptr) {
                    debug!("Opened project in tab {} (ptr={:x}): {}", tab, ptr, path);
                    return Some(project_to_info(&p));
                }
            }
        }

        // Fallback: the project may have loaded into the current tab
        let current = project_to_info(&reaper.current_project());
        debug!("Opened project (current tab fallback): {}", current.name);
        Some(current)
    }

    fn create(&self) -> Option<ProjectInfo> {
        info!("Projects::create");

        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        // Snapshot existing tab pointers before creating
        let mut existing_ptrs = std::collections::HashSet::new();
        for tab in 0..MAX_PROJECT_TABS {
            match project_by_tab(reaper, tab) {
                Some(p) => {
                    existing_ptrs.insert(p.raw().as_ptr() as usize);
                }
                None => break,
            }
        }
        let old_count = existing_ptrs.len() as u32;

        debug!("create: {old_count} existing tabs before action");

        // Fire REAPER action 41929 = "New project tab (ignore default template)"
        let action_new_tab = CommandId::new(41929);
        medium.main_on_command_ex(action_new_tab, 0, ProjectContext::CurrentProject);

        // Find the new tab by scanning for a pointer not in our snapshot
        for tab in 0..MAX_PROJECT_TABS {
            if let Some(p) = project_by_tab(reaper, tab) {
                let ptr = p.raw().as_ptr() as usize;
                if !existing_ptrs.contains(&ptr) {
                    debug!("New project tab at index {} (ptr={:x})", tab, ptr);
                    return Some(project_to_info(&p));
                }
            } else {
                break;
            }
        }

        // Fallback: new tab appears at old_count
        if let Some(p) = project_by_tab(reaper, old_count) {
            debug!("New project tab via fallback at index {}", old_count);
            return Some(project_to_info(&p));
        }

        tracing::warn!("create: could not find new tab, returning current project");
        Some(project_to_info(&reaper.current_project()))
    }

    fn close(&self, project_id: &str) -> bool {
        info!("Projects::close({})", project_id);

        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        // Check if target is already the current project
        let current_info = project_to_info(&reaper.current_project());
        if current_info.guid != project_id {
            // Navigate to the target tab
            let action_next = CommandId::new(40861);
            let mut found = false;
            for _ in 0..MAX_PROJECT_TABS {
                medium.main_on_command_ex(action_next, 0, ProjectContext::CurrentProject);
                let now = project_to_info(&reaper.current_project());
                if now.guid == project_id {
                    found = true;
                    break;
                }
            }
            if !found {
                info!("Projects::close - project {} not found", project_id);
                return false;
            }
        }

        // Close the current tab: action 40860
        // Note: undomaxmem=0 in reaper.ini prevents the "Save changes?"
        // dialog. REAPER must be launched with -cfgfile pointing to the
        // rig-specific ini for this to work.
        let action_close_tab = CommandId::new(40860);
        medium.main_on_command_ex(action_close_tab, 0, ProjectContext::CurrentProject);

        true
    }

    // =========================================================================
    // Undo
    // =========================================================================

    fn begin_undo_block(&self, project: daw_proto::ProjectContext, label: &str) {
        let Some(proj) = resolve_project(&project) else {
            return;
        };
        Reaper::get()
            .medium_reaper()
            .undo_begin_block_2(reaper_medium::ProjectContext::Proj(proj.raw()));
        // Stash label for end_undo_block fallback
        UNDO_LABEL.with(|cell| cell.replace(Some(label.to_string())));
    }

    fn end_undo_block(
        &self,
        project: daw_proto::ProjectContext,
        label: &str,
        scope: Option<daw_proto::UndoScope>,
    ) {
        let Some(proj) = resolve_project(&project) else {
            return;
        };
        // Use the provided label, falling back to whatever was stashed at begin
        let final_label = if !label.is_empty() {
            label.to_string()
        } else {
            UNDO_LABEL
                .with(|cell| cell.take())
                .unwrap_or_else(|| "FTS action".to_string())
        };

        // Convert daw_proto::UndoScope to reaper_medium::UndoScope
        let reaper_scope = scope
            .as_ref()
            .map(convert_undo_scope)
            .unwrap_or(UndoScope::All);

        Reaper::get().medium_reaper().undo_end_block_2(
            reaper_medium::ProjectContext::Proj(proj.raw()),
            final_label.as_str(),
            reaper_scope,
        );
    }

    fn undo(&self, project: daw_proto::ProjectContext) -> bool {
        let Some(proj) = resolve_project(&project) else {
            return false;
        };
        proj.undo()
    }

    fn redo(&self, project: daw_proto::ProjectContext) -> bool {
        let Some(proj) = resolve_project(&project) else {
            return false;
        };
        proj.redo()
    }

    fn last_undo_label(&self, project: daw_proto::ProjectContext) -> Option<String> {
        let proj = resolve_project(&project)?;
        proj.label_of_last_undoable_action()
            .map(|s| s.to_str().to_string())
    }

    fn last_redo_label(&self, project: daw_proto::ProjectContext) -> Option<String> {
        let proj = resolve_project(&project)?;
        proj.label_of_last_redoable_action()
            .map(|s| s.to_str().to_string())
    }

    // =========================================================================
    // Actions / Commands
    // =========================================================================

    fn run_command(&self, project: daw_proto::ProjectContext, command: &str) -> bool {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        // Resolve command string to CommandId (numeric or named)
        let cmd_id = if let Ok(numeric_id) = command.parse::<u32>() {
            CommandId::new(numeric_id)
        } else if let Some(named_id) = medium.named_command_lookup(command) {
            named_id
        } else {
            // Try with underscore prefix (SWS convention)
            let prefixed = format!("_{command}");
            if let Some(named_id) = medium.named_command_lookup(prefixed.as_str()) {
                named_id
            } else {
                debug!("run_command: command not found: {}", command);
                return false;
            }
        };

        // REAPER actions always operate on the "current" project tab,
        // so we must switch to the correct tab before executing
        let proj_ctx = match resolve_project(&project) {
            Some(p) => {
                unsafe {
                    medium.low().SelectProjectInstance(p.raw().as_ptr());
                }
                ProjectContext::Proj(p.raw())
            }
            None => ProjectContext::CurrentProject,
        };

        medium.main_on_command_ex(cmd_id, 0, proj_ctx);
        debug!("run_command: executed '{}'", command);
        true
    }

    // =========================================================================
    // Save
    // =========================================================================

    fn save(&self, project: daw_proto::ProjectContext) {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        let proj_ctx = match resolve_project(&project) {
            Some(p) => ProjectContext::Proj(p.raw()),
            None => ProjectContext::CurrentProject,
        };

        // Action 40026 = "File: Save project"
        medium.main_on_command_ex(CommandId::new(40026), 0, proj_ctx);
    }

    fn save_all(&self) {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        // Action 40897 = "File: Save all projects"
        medium.main_on_command_ex(CommandId::new(40897), 0, ProjectContext::CurrentProject);
    }

    // =========================================================================
    // Project Info
    // =========================================================================

    fn get_project_info_string(&self, project: daw_proto::ProjectContext, key: &str) -> String {
        let reaper = Reaper::get();
        let low = reaper.medium_reaper().low();
        let proj_ctx = match resolve_project(&project) {
            Some(p) => ProjectContext::Proj(p.raw()),
            None => ProjectContext::CurrentProject,
        };
        let key_cstr = std::ffi::CString::new(key).unwrap_or_default();
        let mut buf = [0u8; 4096];
        let buf_ptr = buf.as_mut_ptr() as *mut std::ffi::c_char;
        unsafe {
            low.GetSetProjectInfo_String(proj_ctx.to_raw(), key_cstr.as_ptr(), buf_ptr, false);
            std::ffi::CStr::from_ptr(buf_ptr)
                .to_string_lossy()
                .to_string()
        }
    }

    fn set_project_info_string(&self, project: daw_proto::ProjectContext, key: &str, value: &str) {
        let reaper = Reaper::get();
        let low = reaper.medium_reaper().low();
        let proj_ctx = match resolve_project(&project) {
            Some(p) => ProjectContext::Proj(p.raw()),
            None => ProjectContext::CurrentProject,
        };
        let key_cstr = std::ffi::CString::new(key).unwrap_or_default();
        let value_cstr = std::ffi::CString::new(value).unwrap_or_default();
        unsafe {
            low.GetSetProjectInfo_String(
                proj_ctx.to_raw(),
                key_cstr.as_ptr(),
                value_cstr.as_ptr() as *mut _,
                true,
            );
        }
    }

    fn get_project_info(&self, project: daw_proto::ProjectContext, key: &str) -> f64 {
        let reaper = Reaper::get();
        let low = reaper.medium_reaper().low();
        let proj_ptr = match resolve_project(&project) {
            Some(p) => p.raw().as_ptr(),
            None => reaper.current_project().raw().as_ptr(),
        };
        let key_cstr = std::ffi::CString::new(key).unwrap_or_default();
        unsafe { low.GetSetProjectInfo(proj_ptr, key_cstr.as_ptr(), 0.0, false) }
    }

    fn set_project_info(&self, project: daw_proto::ProjectContext, key: &str, value: f64) {
        let reaper = Reaper::get();
        let low = reaper.medium_reaper().low();
        let proj_ptr = match resolve_project(&project) {
            Some(p) => p.raw().as_ptr(),
            None => reaper.current_project().raw().as_ptr(),
        };
        let key_cstr = std::ffi::CString::new(key).unwrap_or_default();
        unsafe {
            low.GetSetProjectInfo(proj_ptr, key_cstr.as_ptr(), value, true);
        }
    }

    fn get_project_config(&self, project: daw_proto::ProjectContext, key: &str) -> Option<f64> {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let proj_ctx = match resolve_project(&project) {
            Some(p) => ProjectContext::Proj(p.raw()),
            None => ProjectContext::CurrentProject,
        };
        let (ptr, size) = if let Some(descriptor) = medium.project_config_var_get_offs(key)
            && descriptor.offset != 0
        {
            match medium.project_config_var_addr(proj_ctx, descriptor.offset) {
                Some(ptr) => (ptr, descriptor.size),
                None => {
                    tracing::debug!(
                        key,
                        offset = descriptor.offset,
                        "get_project_config: project config address is null, trying global config var"
                    );
                    let global = medium.get_config_var(key)?;
                    (global.value, global.size)
                }
            }
        } else {
            let global = medium.get_config_var(key)?;
            (global.value, global.size)
        };

        let value = unsafe {
            match size {
                size if size == std::mem::size_of::<f64>() as u32 => *(ptr.as_ptr() as *const f64),
                size if size == std::mem::size_of::<f32>() as u32 => {
                    *(ptr.as_ptr() as *const f32) as f64
                }
                size if size == std::mem::size_of::<i32>() as u32 => {
                    *(ptr.as_ptr() as *const i32) as f64
                }
                size if size == std::mem::size_of::<i16>() as u32 => {
                    *(ptr.as_ptr() as *const i16) as f64
                }
                size => {
                    tracing::warn!(
                        key,
                        size,
                        "get_project_config: unsupported project config var size"
                    );
                    return None;
                }
            }
        };
        Some(value)
    }

    fn set_project_config(
        &self,
        project: daw_proto::ProjectContext,
        key: &str,
        value: f64,
    ) -> bool {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let proj_ctx = match resolve_project(&project) {
            Some(p) => ProjectContext::Proj(p.raw()),
            None => ProjectContext::CurrentProject,
        };
        let (ptr, size, is_project_config) = if let Some(descriptor) =
            medium.project_config_var_get_offs(key)
            && descriptor.offset != 0
        {
            match medium.project_config_var_addr(proj_ctx, descriptor.offset) {
                Some(ptr) => (ptr, descriptor.size, true),
                None => {
                    tracing::debug!(
                        key,
                        offset = descriptor.offset,
                        "set_project_config: project config address is null, trying global config var"
                    );
                    let Some(global) = medium.get_config_var(key) else {
                        tracing::warn!(key, "set_project_config: config var not found");
                        return false;
                    };
                    (global.value, global.size, false)
                }
            }
        } else {
            let Some(global) = medium.get_config_var(key) else {
                tracing::warn!(key, "set_project_config: config var not found");
                return false;
            };
            (global.value, global.size, false)
        };

        let written = unsafe {
            match size {
                size if size == std::mem::size_of::<f64>() as u32 => {
                    *(ptr.as_ptr() as *mut f64) = value;
                    true
                }
                size if size == std::mem::size_of::<f32>() as u32 => {
                    *(ptr.as_ptr() as *mut f32) = value as f32;
                    true
                }
                size if size == std::mem::size_of::<i32>() as u32 => {
                    *(ptr.as_ptr() as *mut i32) = value.round() as i32;
                    true
                }
                size if size == std::mem::size_of::<i16>() as u32 => {
                    *(ptr.as_ptr() as *mut i16) = value.round() as i16;
                    true
                }
                size => {
                    tracing::warn!(
                        key,
                        size,
                        "set_project_config: unsupported project config var size"
                    );
                    false
                }
            }
        };

        if written {
            if is_project_config {
                medium.mark_project_dirty(proj_ctx);
            } else {
                let profile_value = config_value_for_profile(key, size, value);
                if !persist_global_config_value(key, &profile_value) {
                    tracing::warn!(
                        key,
                        value = profile_value,
                        "set_project_config: failed to persist global config var"
                    );
                }
            }
            medium.update_timeline();
            if key == "preroll" || key == "prerollmeas" {
                let read_back = self.get_project_config(project, key);
                tracing::info!(
                    key,
                    value,
                    ?read_back,
                    is_project_config,
                    size,
                    "set_project_config: wrote config var"
                );
            }
        }
        written
    }

    // =========================================================================
    // Ruler Lane Management (v7.62+)
    // =========================================================================

    fn set_ruler_lane_name(&self, project: daw_proto::ProjectContext, lane_index: u32, name: &str) {
        let reaper = Reaper::get();
        let low = reaper.medium_reaper().low();
        let proj_ctx = match resolve_project(&project) {
            Some(p) => ProjectContext::Proj(p.raw()),
            None => ProjectContext::CurrentProject,
        };
        // Don't pre-extend via `RULER_LANE_ORDER:N = -1`. That call
        // appears to add *two* lane rows at the tail (an inserted
        // slot for the requested index plus an empty trailing row),
        // which surfaced as a stray "lane 4" after we created
        // SONG / SECTIONS / MARKS. Writing the name on a missing
        // index auto-creates the lane in place, no extra row.
        let key = std::ffi::CString::new(format!("RULER_LANE_NAME:{}", lane_index)).unwrap();
        let value = std::ffi::CString::new(name).unwrap_or_default();
        unsafe {
            low.GetSetProjectInfo_String(
                proj_ctx.to_raw(),
                key.as_ptr(),
                value.as_ptr() as *mut _,
                true,
            );
        }
        low.UpdateTimeline();
    }

    fn get_ruler_lane_name(&self, project: daw_proto::ProjectContext, lane_index: u32) -> String {
        let reaper = Reaper::get();
        let low = reaper.medium_reaper().low();
        let proj_ctx = match resolve_project(&project) {
            Some(p) => ProjectContext::Proj(p.raw()),
            None => ProjectContext::CurrentProject,
        };
        let key = std::ffi::CString::new(format!("RULER_LANE_NAME:{}", lane_index)).unwrap();
        let mut buf = [0u8; 256];
        let buf_ptr = buf.as_mut_ptr() as *mut std::ffi::c_char;
        unsafe {
            low.GetSetProjectInfo_String(proj_ctx.to_raw(), key.as_ptr(), buf_ptr, false);
            std::ffi::CStr::from_ptr(buf_ptr)
                .to_string_lossy()
                .to_string()
        }
    }

    fn ruler_lane_count(&self, project: daw_proto::ProjectContext) -> u32 {
        let reaper = Reaper::get();
        let low = reaper.medium_reaper().low();
        let proj_ctx = match resolve_project(&project) {
            Some(p) => ProjectContext::Proj(p.raw()),
            None => ProjectContext::CurrentProject,
        };
        ruler_lane_count(low, proj_ctx)
    }
}

fn ensure_ruler_lane_exists(
    low: &crate::safe_wrappers::ReaperLow,
    project: ProjectContext,
    lane_index: u32,
) {
    // `lane_index` is 0-based (matches `RULER_LANE_NAME:N` keys).
    // `ruler_lane_count` returns the *number* of named lanes, so we
    // need to insert until count is at least `lane_index + 1` —
    // otherwise asking for lane 2 (third position) with two existing
    // lanes does nothing and the third lane never appears.
    let mut count = ruler_lane_count(low, project).max(1);
    while count <= lane_index {
        // RULER_LANE_ORDER:N is 1-based in REAPER's project-info API.
        // count==2 means we currently have lanes at API indices 0 and
        // 1 (file rows 1 and 2); inserting at row count+1=3 creates
        // the new lane at API index 2.
        let key = std::ffi::CString::new(format!("RULER_LANE_ORDER:{}", count + 1)).unwrap();
        unsafe {
            low.GetSetProjectInfo(project.to_raw(), key.as_ptr(), -1.0, true);
        }
        count += 1;
    }
}

fn ruler_lane_count(low: &crate::safe_wrappers::ReaperLow, project: ProjectContext) -> u32 {
    // Iterate the 0-based name-table index — RULER_LANE_NAME:0 is
    // the leftmost lane, not RULER_LANE_NAME:1. Starting at 1 missed
    // the first lane entirely and undercounted by one, which
    // cascaded into `ensure_ruler_lane_exists` and `hide_stray_lanes`
    // both operating on the wrong slots.
    let mut count = 0u32;
    for i in 0..128 {
        let key = std::ffi::CString::new(format!("RULER_LANE_NAME:{}", i)).unwrap();
        let mut buf = [0u8; 256];
        let buf_ptr = buf.as_mut_ptr() as *mut std::ffi::c_char;
        unsafe {
            low.GetSetProjectInfo_String(project.to_raw(), key.as_ptr(), buf_ptr, false);
            let name = std::ffi::CStr::from_ptr(buf_ptr).to_string_lossy();
            if name.is_empty() {
                break;
            }
            count = i + 1; // number of named lanes ending at this index
        }
    }
    count.max(1)
}
