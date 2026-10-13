//! `impl UiDialogs for Standalone` — stub (headless, dialogs always cancel).

use daw_proto::{UiDialogs, UserInputResult};
use std::path::PathBuf;

use crate::sync::Standalone;

impl UiDialogs for Standalone {
    fn get_user_inputs(
        &self,
        _title: &str,
        _prompts: Vec<String>,
        _defaults: Vec<String>,
    ) -> Option<UserInputResult> {
        None
    }
    fn browse_for_file(
        &self,
        _title: &str,
        _initial_dir: Option<PathBuf>,
        _filter: Option<String>,
    ) -> Option<PathBuf> {
        None
    }
    fn browse_for_save_file(
        &self,
        _title: &str,
        _initial_dir: Option<PathBuf>,
        _default_name: &str,
        _filter: Option<String>,
    ) -> Option<PathBuf> {
        None
    }
    fn browse_for_directory(&self, _title: &str, _initial_dir: Option<PathBuf>) -> Option<PathBuf> {
        None
    }
    fn set_prevent_ui_refresh(&self, _prevent: bool) {}
}
