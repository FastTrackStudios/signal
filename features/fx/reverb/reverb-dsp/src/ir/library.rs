//! Scan a directory for impulse-response files.

use std::path::{Path, PathBuf};

/// Single discovered IR file.
#[derive(Debug, Clone)]
pub struct IrEntry {
    pub path: PathBuf,
    /// Display name — file stem (no extension).
    pub name: String,
    /// File extension, lowercased.
    pub format: String,
}

const SUPPORTED_EXTS: &[&str] = &["wav", "aiff", "aif", "flac", "ogg", "mp3"];

pub struct IrLibrary {
    root: PathBuf,
    entries: Vec<IrEntry>,
}

impl IrLibrary {
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        Self {
            root: root.into(),
            entries: Vec::new(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> &[IrEntry] {
        &self.entries
    }

    /// Walk `root` recursively, collect IR files. Re-running replaces
    /// the entry list.
    pub fn rescan(&mut self) -> std::io::Result<()> {
        self.entries.clear();
        if !self.root.is_dir() {
            return Ok(());
        }
        visit(&self.root, &mut self.entries)?;
        self.entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(())
    }
}

fn visit(dir: &Path, out: &mut Vec<IrEntry>) -> std::io::Result<()> {
    for dent in std::fs::read_dir(dir)? {
        let dent = dent?;
        let path = dent.path();
        if path.is_dir() {
            // Best-effort recurse; skip on permission errors.
            let _ = visit(&path, out);
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { continue };
        let ext_lc = ext.to_ascii_lowercase();
        if !SUPPORTED_EXTS.contains(&ext_lc.as_str()) {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("(unnamed)")
            .to_string();
        out.push(IrEntry { path, name, format: ext_lc });
    }
    Ok(())
}
