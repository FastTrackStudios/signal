use anyhow::{Context, Result, bail};
use resvg::tiny_skia::Pixmap;
use std::path::{Path, PathBuf};

use crate::render::SCALES;

/// Expand a leading `~/`.
pub fn expand(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(p)
}

/// Auto-detect REAPER resource paths (a dir containing reaper.ini).
pub fn detect_resource_paths() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut candidates = Vec::new();
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".fts-dev"));
    }
    if let Some(cfg) = dirs::config_dir() {
        candidates.push(cfg.join("REAPER"));
    }
    for c in candidates {
        if c.join("reaper.ini").is_file() {
            found.push(c);
        }
    }
    found
}

/// Resolve install targets: explicit list (config/CLI) wins, else auto-detect.
pub fn resolve_targets(explicit: &[String]) -> Result<Vec<PathBuf>> {
    if !explicit.is_empty() {
        return explicit
            .iter()
            .map(|p| {
                let p = expand(p);
                if !p.is_dir() {
                    bail!("resource path {} does not exist", p.display());
                }
                Ok(p)
            })
            .collect();
    }
    let found = detect_resource_paths();
    if found.is_empty() {
        bail!(
            "no REAPER resource path found (looked for reaper.ini in ~/.fts-dev and ~/.config/REAPER) — pass --resource-path"
        );
    }
    Ok(found)
}

/// Write the 3 scale variants. `root` is either `<resource>/Data/toolbar_icons`
/// (install mode) or a plain output dir. REAPER wants identical filenames in
/// the base dir and the 150/ + 200/ subfolders — no suffixes.
pub fn write_icon(root: &Path, file: &str, strips: &[Pixmap; 3]) -> Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for ((_, sub), pm) in SCALES.iter().zip(strips) {
        let dir = if sub.is_empty() {
            root.to_path_buf()
        } else {
            root.join(sub)
        };
        std::fs::create_dir_all(&dir).with_context(|| format!("mkdir {}", dir.display()))?;
        let path = dir.join(format!("{file}.png"));
        pm.save_png(&path)
            .with_context(|| format!("write {}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

pub fn toolbar_icons_dir(resource: &Path) -> PathBuf {
    resource.join("Data").join("toolbar_icons")
}
