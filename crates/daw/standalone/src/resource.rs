//! `impl ResourcePaths for Standalone` — post-architect::rpc port.
//!
//! The standalone mock returns deterministic placeholder paths.

use daw_proto::ResourcePaths;

use crate::sync::Standalone;

impl ResourcePaths for Standalone {
    fn resource_path(&self) -> String {
        "/mock/resource".into()
    }

    fn ini_file_path(&self) -> String {
        "/mock/reaper.ini".into()
    }

    fn color_theme_path(&self) -> Option<String> {
        Some("/mock/theme.ReaperTheme".into())
    }

    /// No renderer to reload, so nothing to do — reported as not loaded
    /// rather than faking success, so a caller can tell the difference.
    fn load_color_theme(&self, _path: Option<String>) -> bool {
        false
    }
}
