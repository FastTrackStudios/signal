//! UI service — dialogs and refresh control.

use super::UserInputResult;
use std::path::PathBuf;

#[architect::rpc]
pub trait UiDialogs {
    /// Multi-input dialog. Returns `None` if cancelled.
    fn get_user_inputs(
        &self,
        title: &str,
        prompts: Vec<String>,
        defaults: Vec<String>,
    ) -> Option<UserInputResult>;

    /// File picker for an existing file.
    fn browse_for_file(
        &self,
        title: &str,
        initial_dir: Option<PathBuf>,
        filter: Option<String>,
    ) -> Option<PathBuf>;

    /// File picker for "save as".
    fn browse_for_save_file(
        &self,
        title: &str,
        initial_dir: Option<PathBuf>,
        default_name: &str,
        filter: Option<String>,
    ) -> Option<PathBuf>;

    /// Directory picker.
    fn browse_for_directory(&self, title: &str, initial_dir: Option<PathBuf>) -> Option<PathBuf>;

    /// Temporarily suspend UI refresh for batch operations.
    fn set_prevent_ui_refresh(&self, prevent: bool);
}
