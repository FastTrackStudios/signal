//! `impl ResourcePaths for Reaper` — sync trait + REAPER's resource-path C API.
//!
//! Mounting goes through `daw_proto::resource::serve(Reaper)`. The
//! architect::rpc bridge hops calls onto REAPER's main thread via
//! `HasDispatcher`. All bodies assume main-thread execution.

use daw_proto::ResourcePaths;
use reaper_high::Reaper as ReaperHigh;

use crate::safe_wrappers::cstring;

impl ResourcePaths for crate::Reaper {
    fn resource_path(&self) -> String {
        let reaper = ReaperHigh::get();
        let medium = reaper.medium_reaper();
        medium.get_resource_path(|p| p.to_string())
    }

    fn ini_file_path(&self) -> String {
        let reaper = ReaperHigh::get();
        let medium = reaper.medium_reaper();
        medium.get_ini_file(|p| p.to_string())
    }

    fn color_theme_path(&self) -> Option<String> {
        let reaper = ReaperHigh::get();
        let low = reaper.medium_reaper().low();
        // GetLastColorThemeFile returns a C string pointer
        let ptr = low.GetLastColorThemeFile();
        cstring::read_cstr(ptr)
    }

    fn load_color_theme(&self, path: Option<String>) -> bool {
        // No path = reload the active theme, which is what a themer wants
        // almost every time. REAPER re-reads rtconfig.txt and the image
        // folder on every open, so re-opening the current file IS a reload.
        let Some(path) = path.or_else(|| self.color_theme_path()) else {
            // Default theme active and nothing named: nothing to re-open.
            return false;
        };
        let Ok(c_path) = std::ffi::CString::new(path) else {
            // An interior NUL can't reach REAPER's C API.
            return false;
        };

        let reaper = ReaperHigh::get();
        let low = reaper.medium_reaper().low();
        // SAFETY: `c_path` is a valid NUL-terminated string that outlives the
        // call, and this runs on REAPER's main thread (the rpc bridge hops
        // there before dispatch), which is where theme loading must happen.
        unsafe { low.OpenColorThemeFile(c_path.as_ptr()) }
    }
}
