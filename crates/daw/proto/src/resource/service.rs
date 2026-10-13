//! REAPER resource-path service.
//!
//! Paths cross the wire as `String`, not `PathBuf`: the vox/phon derive has no
//! encoding for `PathBuf`, and a trait returning one compiles fine but fails
//! at call time with "cannot derive phon from this type". Callers get real
//! `PathBuf`s back from the `daw-control` handle, which converts.

#[architect::rpc]
pub trait ResourcePaths {
    /// REAPER resource directory (presets, templates, themes, …).
    fn resource_path(&self) -> String;

    /// Path to REAPER's `reaper.ini` configuration file.
    fn ini_file_path(&self) -> String;

    /// Path to the currently loaded color theme file. `None` when the
    /// default theme is active.
    fn color_theme_path(&self) -> Option<String>;

    /// (Re)load a color theme from disk, returning whether it took.
    ///
    /// `None` reloads whatever is already active — the theme-development
    /// loop: edit `rtconfig.txt` or the palette, call this, see it.
    ///
    /// REAPER has no "reload theme" *action* to trigger (7.59 ships only the
    /// element finder and the tweak window), so the `OpenColorThemeFile` API
    /// behind this is the only way to drive a reload from outside REAPER.
    fn load_color_theme(&self, path: Option<String>) -> bool;
}
