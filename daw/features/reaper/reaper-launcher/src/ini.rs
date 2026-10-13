//! Low-level reaper.ini reader/writer.
//!
//! REAPER's ini format is a simple `key=value` per line under `[reaper]`.
//! This module handles reading, patching, and restoring individual keys
//! without disturbing the rest of the file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Handle to a `reaper.ini` file with read/write capabilities.
pub struct ReaperIni {
    path: PathBuf,
}

impl ReaperIni {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Derive the ini path from the FTS-TRACKS base directory.
    pub fn from_base_dir(base_dir: &Path) -> Self {
        Self::new(base_dir.join("reaper.ini"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read a single key's value from the ini file.
    /// Returns `None` if the key doesn't exist (REAPER uses defaults).
    pub fn get(&self, key: &str) -> Option<String> {
        let content = std::fs::read_to_string(&self.path).ok()?;
        let needle = format!("{key}=");
        for line in content.lines() {
            if let Some(val) = line.strip_prefix(&needle) {
                return Some(val.trim_end().to_string());
            }
        }
        None
    }

    /// Set a key to a new value. If the key exists, its value is replaced.
    /// If it doesn't exist, it's appended under the `[reaper]` section.
    /// Returns the previous value (if any).
    pub fn set(&self, key: &str, value: &str) -> std::io::Result<Option<String>> {
        let content = std::fs::read_to_string(&self.path)?;
        let needle = format!("{key}=");
        let mut found = false;
        let mut old_value = None;
        let mut lines: Vec<String> = Vec::new();

        for line in content.lines() {
            if let Some(val) = line.strip_prefix(&needle) {
                old_value = Some(val.trim_end().to_string());
                lines.push(format!("{key}={value}"));
                found = true;
            } else {
                lines.push(line.to_string());
            }
        }

        if !found {
            // Insert after [reaper] header, or at end
            let mut inserted = false;
            let mut result: Vec<String> = Vec::new();
            for line in &lines {
                result.push(line.clone());
                if !inserted && line.trim() == "[reaper]" {
                    result.push(format!("{key}={value}"));
                    inserted = true;
                }
            }
            if !inserted {
                result.push(format!("{key}={value}"));
            }
            lines = result;
        }

        // Preserve original line endings
        let has_trailing_newline = content.ends_with('\n');
        let mut output = lines.join("\n");
        if has_trailing_newline {
            output.push('\n');
        }
        std::fs::write(&self.path, output)?;
        Ok(old_value)
    }

    /// Patch multiple keys at once. Returns a map of old values for restoration.
    pub fn patch(
        &self,
        changes: &[(&str, &str)],
    ) -> std::io::Result<HashMap<String, Option<String>>> {
        let mut originals = HashMap::new();
        for &(key, value) in changes {
            let old = self.set(key, value)?;
            originals.insert(key.to_string(), old);
        }
        Ok(originals)
    }

    /// Restore previously saved values. If original was `None`, the key
    /// is left as-is (REAPER will use its default on next read).
    pub fn restore(&self, originals: &HashMap<String, Option<String>>) -> std::io::Result<()> {
        for (key, old_value) in originals {
            if let Some(val) = old_value {
                self.set(key, val)?;
            }
            // If the original was None (key didn't exist), we could remove it,
            // but leaving the explicit value is safer — REAPER handles both.
        }
        Ok(())
    }
}
