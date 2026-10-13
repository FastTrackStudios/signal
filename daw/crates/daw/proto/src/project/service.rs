//! Projects service — list/open/close/save + per-project info/undo/
//! commands/ruler-lanes. Subscriber stream retires (sibling trait).

use super::{ProjectContext, ProjectInfo};
use crate::batch::ProjectArg;
use crate::{DawResult, UndoScope};

#[architect::rpc(ops(ProjectContext as ProjectArg))]
pub trait Projects {
    // ── Per-project handle ──────────────────────────────────────────
    //
    // Pre-port, this lived on a separate sync `Project` per-project
    // handle trait. With everything using `ProjectContext` per call
    // the handle pattern retires; `info` covers what `Project::info`
    // returned.

    /// Project metadata for `project`.
    fn info(&self, project: ProjectContext) -> DawResult<ProjectInfo>;

    // ── Collection ─────────────────────────────────────────────────

    fn current(&self) -> Option<ProjectInfo>;
    fn get(&self, project_id: &str) -> Option<ProjectInfo>;
    fn list(&self) -> Vec<ProjectInfo>;

    /// Project at the given 0-based tab slot, if any.
    fn get_by_slot(&self, slot: u32) -> Option<ProjectInfo>;

    /// Make `project_id` the focused project. Returns true if found.
    fn select(&self, project_id: &str) -> bool;

    /// Open an existing `.rpp` file in a new tab.
    fn open(&self, path: &str) -> Option<ProjectInfo>;

    /// Create a new empty project tab.
    fn create(&self) -> Option<ProjectInfo>;

    /// Close a specific project tab by GUID. True if found+closed.
    fn close(&self, project_id: &str) -> bool;

    // ── Undo ──────────────────────────────────────────────────────

    fn begin_undo_block(&self, project: ProjectContext, label: &str);
    fn end_undo_block(&self, project: ProjectContext, label: &str, scope: Option<UndoScope>);

    fn undo(&self, project: ProjectContext) -> bool;
    fn redo(&self, project: ProjectContext) -> bool;

    fn last_undo_label(&self, project: ProjectContext) -> Option<String>;
    fn last_redo_label(&self, project: ProjectContext) -> Option<String>;

    // ── Commands ──────────────────────────────────────────────────

    /// Run a REAPER action/command by string id (named or numeric).
    fn run_command(&self, project: ProjectContext, command: &str) -> bool;

    // ── Save ──────────────────────────────────────────────────────

    fn save(&self, project: ProjectContext);
    /// Save all open projects (REAPER action 40897).
    fn save_all(&self);

    // ── Project info (GetSetProjectInfo / _String) ────────────────

    fn get_project_info_string(&self, project: ProjectContext, key: &str) -> String;
    fn set_project_info_string(&self, project: ProjectContext, key: &str, value: &str);
    fn get_project_info(&self, project: ProjectContext, key: &str) -> f64;
    fn set_project_info(&self, project: ProjectContext, key: &str, value: f64);
    fn get_project_config(&self, project: ProjectContext, key: &str) -> Option<f64>;
    fn set_project_config(&self, project: ProjectContext, key: &str, value: f64) -> bool;

    // ── Ruler lanes (v7.62+) ──────────────────────────────────────

    fn set_ruler_lane_name(&self, project: ProjectContext, lane_index: u32, name: &str);
    fn get_ruler_lane_name(&self, project: ProjectContext, lane_index: u32) -> String;
    fn ruler_lane_count(&self, project: ProjectContext) -> u32;
}
