//! `impl ExtState for Reaper` — sync trait + REAPER's ext-state C API.
//!
//! Mounting goes through `daw_proto::ext_state::serve(Reaper)`. The
//! architect::rpc bridge hops calls onto REAPER's main thread via
//! `HasDispatcher`. All bodies assume main-thread execution.

use std::ffi::CString;

use daw_proto::ExtState;
use daw_proto::{DawError, DawResult, ProjectContext};
use reaper_high::Reaper as ReaperHigh;

use crate::project_context::resolve_project_context;
use crate::safe_wrappers::ext_state as sw;

fn cstr(s: &str) -> DawResult<CString> {
    CString::new(s).map_err(|e| DawError::operation_failed(format!("invalid string: {e}")))
}

impl ExtState for crate::Reaper {
    fn get(&self, section: &str, key: &str) -> Option<String> {
        let section_c = CString::new(section).ok()?;
        let key_c = CString::new(key).ok()?;
        let low = ReaperHigh::get().medium_reaper().low();
        sw::get_ext_state(low, &section_c, &key_c)
    }

    fn set(&self, section: &str, key: &str, value: &str, persist: bool) -> DawResult<()> {
        let section_c = cstr(section)?;
        let key_c = cstr(key)?;
        let value_c = cstr(value)?;
        let low = ReaperHigh::get().medium_reaper().low();
        sw::set_ext_state(low, &section_c, &key_c, &value_c, persist);
        Ok(())
    }

    fn delete(&self, section: &str, key: &str, persist: bool) -> DawResult<()> {
        let section_c = cstr(section)?;
        let key_c = cstr(key)?;
        let low = ReaperHigh::get().medium_reaper().low();
        sw::delete_ext_state(low, &section_c, &key_c, persist);
        Ok(())
    }

    fn has(&self, section: &str, key: &str) -> bool {
        let Ok(section_c) = CString::new(section) else {
            return false;
        };
        let Ok(key_c) = CString::new(key) else {
            return false;
        };
        let low = ReaperHigh::get().medium_reaper().low();
        sw::has_ext_state(low, &section_c, &key_c)
    }

    fn get_project(&self, project: ProjectContext, section: &str, key: &str) -> Option<String> {
        let section_c = CString::new(section).ok()?;
        let key_c = CString::new(key).ok()?;
        let low = ReaperHigh::get().medium_reaper().low();
        let proj_ctx = resolve_project_context(&project);
        sw::get_proj_ext_state(low, proj_ctx, &section_c, &key_c, 4096)
    }

    fn set_project(
        &self,
        project: ProjectContext,
        section: &str,
        key: &str,
        value: &str,
    ) -> DawResult<()> {
        let section_c = cstr(section)?;
        let key_c = cstr(key)?;
        let value_c = cstr(value)?;
        let low = ReaperHigh::get().medium_reaper().low();
        let proj_ctx = resolve_project_context(&project);
        sw::set_proj_ext_state(low, proj_ctx, &section_c, &key_c, &value_c);
        Ok(())
    }

    fn delete_project(&self, project: ProjectContext, section: &str, key: &str) -> DawResult<()> {
        // REAPER convention: setting to "" deletes.
        ExtState::set_project(self, project, section, key, "")
    }

    fn has_project(&self, project: ProjectContext, section: &str, key: &str) -> bool {
        ExtState::get_project(self, project, section, key).is_some()
    }
}
