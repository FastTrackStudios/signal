//! `impl UiDialogs for Reaper` — sync trait + REAPER's dialog/SWELL helpers.

use daw_proto::{UiDialogs, UserInputResult};
use reaper_high::Reaper;
use std::path::PathBuf;

impl UiDialogs for crate::Reaper {
    fn get_user_inputs(
        &self,
        title: &str,
        prompts: Vec<String>,
        defaults: Vec<String>,
    ) -> Option<UserInputResult> {
        let medium = Reaper::get().medium_reaper();
        if prompts.len() > 16 || prompts.len() != defaults.len() {
            return None;
        }
        let num_inputs = prompts.len() as u32;
        let captions = prompts.join(",");
        let initial_csv = defaults.join(",");
        let result =
            medium.get_user_inputs(title.to_string(), num_inputs, captions, initial_csv, 512);
        match result {
            Some(values_str) => {
                let values: Vec<String> = values_str
                    .to_str()
                    .split(',')
                    .map(|s| s.to_string())
                    .collect();
                Some(UserInputResult { ok: true, values })
            }
            None => Some(UserInputResult {
                ok: false,
                values: vec![],
            }),
        }
    }

    fn browse_for_file(
        &self,
        title: &str,
        initial_dir: Option<PathBuf>,
        filter: Option<String>,
    ) -> Option<PathBuf> {
        if !reaper_low::Swell::is_available_globally() {
            return None;
        }
        let swell = reaper_low::Swell::get();
        crate::safe_wrappers::ui::browse_for_open_file(
            swell,
            title,
            initial_dir.as_deref(),
            filter.as_deref(),
        )
    }

    fn browse_for_save_file(
        &self,
        title: &str,
        initial_dir: Option<PathBuf>,
        default_name: &str,
        filter: Option<String>,
    ) -> Option<PathBuf> {
        if !reaper_low::Swell::is_available_globally() {
            return None;
        }
        let swell = reaper_low::Swell::get();
        crate::safe_wrappers::ui::browse_for_save_file(
            swell,
            title,
            initial_dir.as_deref(),
            default_name,
            filter.as_deref(),
        )
    }

    fn browse_for_directory(&self, title: &str, initial_dir: Option<PathBuf>) -> Option<PathBuf> {
        if !reaper_low::Swell::is_available_globally() {
            return None;
        }
        let swell = reaper_low::Swell::get();
        crate::safe_wrappers::ui::browse_for_directory(swell, title, initial_dir.as_deref())
    }

    fn set_prevent_ui_refresh(&self, prevent: bool) {
        let low = Reaper::get().medium_reaper().low();
        low.PreventUIRefresh(if prevent { 1 } else { 0 });
    }
}
