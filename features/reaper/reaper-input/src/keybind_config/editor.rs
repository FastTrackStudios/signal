//! Programmatic config editing — read, modify, and write .styx keybind files.
//!
//! Provides an API for the keyboard visualizer to:
//! - List all bindings from a section file
//! - Add/remove/update individual bindings
//! - Write changes back to .styx files
//! - Trigger hot-reload of the config
//!
//! Uses facet-styx for round-trip serialization.

use std::path::PathBuf;

use crate::keybind_config::types::{
    KeybindDef, MouseBindDef, ProfileConfig, SectionConfig, WheelBindDef,
};

// ---------------------------------------------------------------------------
// Config editor
// ---------------------------------------------------------------------------

/// Represents an editable section config file.
pub struct SectionEditor {
    /// Path to the .styx file on disk.
    pub path: PathBuf,
    /// Parsed config (mutable).
    pub config: SectionConfig,
    /// Whether changes have been made since last save.
    dirty: bool,
}

impl SectionEditor {
    /// Load a section config from a .styx file.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, EditorError> {
        let path = path.into();
        let content =
            std::fs::read_to_string(&path).map_err(|e| EditorError::Io(path.clone(), e))?;
        let config: SectionConfig = facet_styx::from_str(&content)
            .map_err(|e| EditorError::Parse(path.clone(), format!("{e}")))?;
        Ok(Self {
            path,
            config,
            dirty: false,
        })
    }

    /// Get all keyboard bindings.
    pub fn bindings(&self) -> &[KeybindDef] {
        self.config.bindings()
    }

    /// Add a new keyboard binding.
    pub fn add_binding(&mut self, binding: KeybindDef) {
        let bindings = self.config.bindings.get_or_insert_with(Vec::new);
        bindings.push(binding);
        self.dirty = true;
    }

    /// Remove a binding by key sequence string.
    /// Returns the removed binding, or None if not found.
    pub fn remove_binding(&mut self, keys: &str) -> Option<KeybindDef> {
        let bindings = self.config.bindings.as_mut()?;
        let pos = bindings.iter().position(|b| b.keys == keys)?;
        self.dirty = true;
        Some(bindings.remove(pos))
    }

    /// Update an existing binding's action and description.
    /// Returns false if the binding wasn't found.
    pub fn update_binding(&mut self, keys: &str, action: String, desc: Option<String>) -> bool {
        let Some(bindings) = self.config.bindings.as_mut() else {
            return false;
        };
        if let Some(binding) = bindings.iter_mut().find(|b| b.keys == keys) {
            binding.action = action;
            binding.desc = desc;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Get all wheel bindings.
    pub fn wheel(&self) -> &[WheelBindDef] {
        self.config.wheel()
    }

    /// Add a wheel binding.
    pub fn add_wheel(&mut self, bind: WheelBindDef) {
        self.config.wheel.get_or_insert_with(Vec::new).push(bind);
        self.dirty = true;
    }

    /// Remove a wheel binding by index. Returns the removed binding.
    pub fn remove_wheel(&mut self, index: usize) -> Option<WheelBindDef> {
        let wheel = self.config.wheel.as_mut()?;
        if index >= wheel.len() {
            return None;
        }
        self.dirty = true;
        Some(wheel.remove(index))
    }

    /// Get all mouse click/drag modifier bindings.
    pub fn mouse(&self) -> &[MouseBindDef] {
        self.config.mouse()
    }

    /// Add a mouse binding.
    pub fn add_mouse(&mut self, bind: MouseBindDef) {
        self.config.mouse.get_or_insert_with(Vec::new).push(bind);
        self.dirty = true;
    }

    /// Remove a mouse binding by index. Returns the removed binding.
    pub fn remove_mouse(&mut self, index: usize) -> Option<MouseBindDef> {
        let mouse = self.config.mouse.as_mut()?;
        if index >= mouse.len() {
            return None;
        }
        self.dirty = true;
        Some(mouse.remove(index))
    }

    /// Check if there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Save changes back to the .styx file (with undo snapshot).
    pub fn save(&mut self) -> Result<(), EditorError> {
        // Snapshot for undo before overwriting
        crate::keybind_config::undo::snapshot(&self.path, "config edit");
        let content = facet_styx::to_string(&self.config)
            .map_err(|e| EditorError::Serialize(self.path.clone(), format!("{e}")))?;
        std::fs::write(&self.path, &content).map_err(|e| EditorError::Io(self.path.clone(), e))?;
        self.dirty = false;
        tracing::info!(path = %self.path.display(), "Saved section config");
        Ok(())
    }

    /// Save and trigger config hot-reload.
    pub fn save_and_reload(&mut self) -> Result<(), EditorError> {
        self.save()?;
        // The config watcher will pick up the file change and reload
        // within one timer tick (~33ms). No explicit action needed.
        tracing::info!("Config change will be hot-reloaded on next tick");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Profile-level editor
// ---------------------------------------------------------------------------

/// Editor for an entire profile (all section files).
pub struct ProfileEditor {
    /// Profile directory path.
    pub dir: PathBuf,
    /// Section file names (in load order from profile.styx).
    pub sections: Vec<String>,
}

impl ProfileEditor {
    /// Open a profile from its directory.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, EditorError> {
        let dir = dir.into();
        let profile_path = dir.join("profile.styx");
        let content = std::fs::read_to_string(&profile_path)
            .map_err(|e| EditorError::Io(profile_path.clone(), e))?;
        let profile: crate::keybind_config::types::ProfileConfig =
            facet_styx::from_str(&content)
                .map_err(|e| EditorError::Parse(profile_path.clone(), format!("{e}")))?;

        Ok(Self {
            dir,
            sections: profile.sections,
        })
    }

    /// Open a specific section file for editing.
    pub fn open_section(&self, section_name: &str) -> Result<SectionEditor, EditorError> {
        let path = self.dir.join(section_name);
        SectionEditor::open(path)
    }

    /// List all section file names.
    pub fn section_names(&self) -> &[String] {
        &self.sections
    }

    /// Find which section file contains a binding for the given key sequence.
    pub fn find_binding_section(&self, keys: &str) -> Option<String> {
        for section_name in &self.sections {
            let path = self.dir.join(section_name);
            if let Ok(editor) = SectionEditor::open(&path)
                && editor.bindings().iter().any(|b| b.keys == keys)
            {
                return Some(section_name.clone());
            }
        }
        None
    }

    /// Collect all bindings across all sections (for display).
    pub fn all_bindings(&self) -> Vec<(String, KeybindDef)> {
        let mut all = Vec::new();
        for section_name in &self.sections {
            let path = self.dir.join(section_name);
            if let Ok(editor) = SectionEditor::open(&path) {
                for binding in editor.bindings() {
                    all.push((section_name.clone(), binding.clone()));
                }
            }
        }
        all
    }
}

// ---------------------------------------------------------------------------
// Profile metadata editor
// ---------------------------------------------------------------------------

/// Read/write editor for a profile's `profile.styx` (name, description,
/// version, and the ordered list of section files).
pub struct ProfileMetaEditor {
    /// Path to `profile.styx`.
    pub path: PathBuf,
    /// Parsed profile config (mutable).
    pub config: ProfileConfig,
    dirty: bool,
}

impl ProfileMetaEditor {
    /// Open the `profile.styx` inside a profile directory.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, EditorError> {
        let path = dir.into().join("profile.styx");
        let content =
            std::fs::read_to_string(&path).map_err(|e| EditorError::Io(path.clone(), e))?;
        let config: ProfileConfig = facet_styx::from_str(&content)
            .map_err(|e| EditorError::Parse(path.clone(), format!("{e}")))?;
        Ok(Self {
            path,
            config,
            dirty: false,
        })
    }

    pub fn set_name(&mut self, name: String) {
        self.config.name = name;
        self.dirty = true;
    }

    pub fn set_description(&mut self, description: String) {
        self.config.description = description;
        self.dirty = true;
    }

    pub fn set_version(&mut self, version: String) {
        self.config.version = version;
        self.dirty = true;
    }

    /// Move a section file up (`delta = -1`) or down (`delta = 1`) in load order.
    pub fn move_section(&mut self, index: usize, delta: isize) {
        let len = self.config.sections.len();
        if len == 0 {
            return;
        }
        let target = index as isize + delta;
        if index >= len || target < 0 || target as usize >= len {
            return;
        }
        self.config.sections.swap(index, target as usize);
        self.dirty = true;
    }

    /// Add a section file to the load order if not already present.
    pub fn add_section(&mut self, filename: String) {
        if !self.config.sections.contains(&filename) {
            self.config.sections.push(filename);
            self.dirty = true;
        }
    }

    /// Remove a section file from the load order.
    pub fn remove_section(&mut self, filename: &str) {
        let before = self.config.sections.len();
        self.config.sections.retain(|s| s != filename);
        if self.config.sections.len() != before {
            self.dirty = true;
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Save back to `profile.styx` (with undo snapshot).
    pub fn save(&mut self) -> Result<(), EditorError> {
        crate::keybind_config::undo::snapshot(&self.path, "profile edit");
        let content = facet_styx::to_string(&self.config)
            .map_err(|e| EditorError::Serialize(self.path.clone(), format!("{e}")))?;
        std::fs::write(&self.path, &content).map_err(|e| EditorError::Io(self.path.clone(), e))?;
        self.dirty = false;
        tracing::info!(path = %self.path.display(), "Saved profile metadata");
        Ok(())
    }
}

/// Create a new, empty profile directory under `keybinds_dir`.
///
/// Writes a `profile.styx` (with one empty `bindings.styx` section) and the
/// section file itself. `id` is the directory name; `name` is the display name.
pub fn create_profile(
    keybinds_dir: impl Into<PathBuf>,
    id: &str,
    name: &str,
) -> Result<PathBuf, EditorError> {
    let dir = keybinds_dir.into().join(id);
    if dir.join("profile.styx").exists() {
        return Err(EditorError::Parse(
            dir.join("profile.styx"),
            "profile already exists".into(),
        ));
    }
    std::fs::create_dir_all(&dir).map_err(|e| EditorError::Io(dir.clone(), e))?;

    let profile = ProfileConfig {
        name: name.to_string(),
        description: String::new(),
        version: "0.1.0".into(),
        sections: vec!["bindings.styx".into()],
    };
    let profile_path = dir.join("profile.styx");
    let content = facet_styx::to_string(&profile)
        .map_err(|e| EditorError::Serialize(profile_path.clone(), format!("{e}")))?;
    std::fs::write(&profile_path, &content)
        .map_err(|e| EditorError::Io(profile_path.clone(), e))?;

    // Empty section file so the profile loads cleanly.
    let section = SectionConfig::default();
    let section_path = dir.join("bindings.styx");
    let section_content = facet_styx::to_string(&section)
        .map_err(|e| EditorError::Serialize(section_path.clone(), format!("{e}")))?;
    std::fs::write(&section_path, &section_content)
        .map_err(|e| EditorError::Io(section_path.clone(), e))?;

    tracing::info!(dir = %dir.display(), "Created profile");
    Ok(dir)
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum EditorError {
    Io(PathBuf, std::io::Error),
    Parse(PathBuf, String),
    Serialize(PathBuf, String),
}

impl std::fmt::Display for EditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(path, e) => write!(f, "{}: {e}", path.display()),
            Self::Parse(path, e) => write!(f, "parse error in {}: {e}", path.display()),
            Self::Serialize(path, e) => write!(f, "serialize error for {}: {e}", path.display()),
        }
    }
}

impl std::error::Error for EditorError {}
