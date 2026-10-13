//! `impl ExtState for Standalone` — post-architect::rpc port.
//!
//! Global keys live in `StandaloneState::global_ext_state`. Project
//! keys live in `ProjectState::project_ext_state`. The `persist` flag
//! is ignored — the standalone mock is transient.

use daw_proto::ExtState;
use daw_proto::{DawError, DawResult, ProjectContext};

use crate::sync::Standalone;

fn resolve_project(daw: &Standalone, ctx: &ProjectContext) -> Option<String> {
    match ctx {
        ProjectContext::Project(guid) => Some(guid.clone()),
        ProjectContext::Current => {
            let state = daw.state.lock().ok()?;
            state.current_project_guid.clone()
        }
    }
}

fn no_project() -> DawError {
    DawError::not_found("Project", "context")
}

impl ExtState for Standalone {
    fn get(&self, section: &str, key: &str) -> Option<String> {
        let state = self.state.lock().ok()?;
        state
            .global_ext_state
            .get(&(section.to_string(), key.to_string()))
            .cloned()
    }

    fn set(&self, section: &str, key: &str, value: &str, _persist: bool) -> DawResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DawError::operation_failed("standalone state lock poisoned"))?;
        state
            .global_ext_state
            .insert((section.to_string(), key.to_string()), value.to_string());
        Ok(())
    }

    fn delete(&self, section: &str, key: &str, _persist: bool) -> DawResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DawError::operation_failed("standalone state lock poisoned"))?;
        state
            .global_ext_state
            .remove(&(section.to_string(), key.to_string()));
        Ok(())
    }

    fn has(&self, section: &str, key: &str) -> bool {
        let Ok(state) = self.state.lock() else {
            return false;
        };
        state
            .global_ext_state
            .contains_key(&(section.to_string(), key.to_string()))
    }

    fn get_project(&self, project: ProjectContext, section: &str, key: &str) -> Option<String> {
        let guid = resolve_project(self, &project)?;
        self.with_project(&guid, |p| {
            p.project_ext_state
                .get(&(section.to_string(), key.to_string()))
                .cloned()
        })
        .ok()
        .flatten()
    }

    fn set_project(
        &self,
        project: ProjectContext,
        section: &str,
        key: &str,
        value: &str,
    ) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(no_project)?;
        self.with_project_mut(&guid, |p| {
            p.project_ext_state
                .insert((section.to_string(), key.to_string()), value.to_string());
        })
    }

    fn delete_project(&self, project: ProjectContext, section: &str, key: &str) -> DawResult<()> {
        let guid = resolve_project(self, &project).ok_or_else(no_project)?;
        self.with_project_mut(&guid, |p| {
            p.project_ext_state
                .remove(&(section.to_string(), key.to_string()));
        })
    }

    fn has_project(&self, project: ProjectContext, section: &str, key: &str) -> bool {
        let Some(guid) = resolve_project(self, &project) else {
            return false;
        };
        self.with_project(&guid, |p| {
            p.project_ext_state
                .contains_key(&(section.to_string(), key.to_string()))
        })
        .unwrap_or(false)
    }
}
