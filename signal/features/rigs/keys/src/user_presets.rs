//! **Saved presets** — the engine and layer presets a player keeps, beside
//! the scanned packs and patches in the browser.
//!
//! A layer preset is one lane as it sounded when saved: its modules' sources,
//! the knobs set on each, its own Tone / Limiter / FX settings and transpose.
//! An engine preset is a whole engine's lanes. Where a preset lands decides
//! the rest — its key range, its fader and its name belong to the slot it is
//! loaded into, not to the preset.
//!
//! One file per preset, the same way the guitar rig keeps its module and
//! block presets:
//!
//! ```text
//! ~/.config/signal/keys/presets/engines/<name>.styx
//! ~/.config/signal/keys/presets/layers/<name>.styx
//! ```

use std::path::{Path, PathBuf};

use facet::Facet;
use signal_keys_proto::KeysPreset;

use crate::profile::{EngineDef, LayerDef};

/// Which kind of saved preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Engine,
    Layer,
}

impl Kind {
    const fn dir(self) -> &'static str {
        match self {
            Self::Engine => "engines",
            Self::Layer => "layers",
        }
    }

    /// The browser scope a preset of this kind sits at.
    #[must_use]
    pub const fn scope(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::Layer => "layer",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Engine => "Engine preset",
            Self::Layer => "Layer preset",
        }
    }
}

/// A saved layer preset on disk.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct LayerPreset {
    pub name: String,
    /// The engine it was saved from ("Keys", "Pad") — the browser's engine
    /// filter.
    #[facet(default)]
    pub engine: String,
    pub layer: LayerDef,
}

/// A saved engine preset on disk.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct EnginePreset {
    pub name: String,
    pub engine: EngineDef,
}

/// Where saved presets live: `$XDG_CONFIG_HOME|~/.config/signal/keys/presets`.
#[must_use]
pub fn root() -> PathBuf {
    signal_rig_host::store::signal_config_dir().join("keys/presets")
}

/// The file a preset named `name` of `kind` is kept in.
#[must_use]
pub fn path_for(kind: Kind, name: &str) -> PathBuf {
    root().join(kind.dir()).join(format!("{}.styx", file_stem(name)))
}

/// A name made safe as a file name (path separators and the like become
/// `-`); the preset keeps its real name inside the file.
fn file_stem(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    if s.is_empty() { "Untitled".to_string() } else { s }
}

/// Which kind a saved preset's file is, from where it lives.
#[must_use]
pub fn kind_of(path: &Path) -> Option<Kind> {
    let dir = path.parent()?;
    if dir.parent()? != root() {
        return None;
    }
    match dir.file_name()?.to_str()? {
        "engines" => Some(Kind::Engine),
        "layers" => Some(Kind::Layer),
        _ => None,
    }
}

fn write(path: &Path, text: Result<String, String>) -> Result<(), String> {
    let text = text?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Save a layer preset; returns its file.
///
/// # Errors
/// When the file cannot be written.
pub fn save_layer(preset: &LayerPreset) -> Result<PathBuf, String> {
    let path = path_for(Kind::Layer, &preset.name);
    write(&path, facet_styx::to_string(preset).map_err(|e| e.to_string()))?;
    Ok(path)
}

/// Save an engine preset; returns its file.
///
/// # Errors
/// When the file cannot be written.
pub fn save_engine(preset: &EnginePreset) -> Result<PathBuf, String> {
    let path = path_for(Kind::Engine, &preset.name);
    write(&path, facet_styx::to_string(preset).map_err(|e| e.to_string()))?;
    Ok(path)
}

/// Read a saved layer preset.
///
/// # Errors
/// When the file is missing or unreadable.
pub fn read_layer(path: &Path) -> Result<LayerPreset, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    facet_styx::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read a saved engine preset.
///
/// # Errors
/// When the file is missing or unreadable.
pub fn read_engine(path: &Path) -> Result<EnginePreset, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    facet_styx::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Give saved preset `path` a new name (its file follows); returns the new
/// file. `copy` keeps the original (Duplicate).
///
/// # Errors
/// When the preset cannot be read or the new file written.
pub fn rename(path: &Path, name: &str, copy: bool) -> Result<PathBuf, String> {
    let kind = kind_of(path).ok_or_else(|| format!("{} is not a saved preset", path.display()))?;
    let new = match kind {
        Kind::Layer => {
            let mut p = read_layer(path)?;
            p.name = name.trim().to_string();
            save_layer(&p)?
        }
        Kind::Engine => {
            let mut p = read_engine(path)?;
            p.name = name.trim().to_string();
            save_engine(&p)?
        }
    };
    if !copy && new != path {
        std::fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(new)
}

/// Delete saved preset `path`.
///
/// # Errors
/// When the file cannot be removed.
pub fn delete(path: &Path) -> Result<(), String> {
    kind_of(path).ok_or_else(|| format!("{} is not a saved preset", path.display()))?;
    std::fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The browser row for a saved preset.
fn row(kind: Kind, name: String, engine: &str) -> KeysPreset {
    KeysPreset {
        name,
        kind: kind.label().to_string(),
        loaded: false,
        scope: kind.scope().to_string(),
        tags: if engine.is_empty() {
            Vec::new()
        } else {
            vec![engine.to_string()]
        },
        variants: Vec::new(),
        user: true,
    }
}

/// Every saved preset, as browser rows with their files (engines first, each
/// kind by name).
#[must_use]
pub fn scan() -> (Vec<KeysPreset>, Vec<PathBuf>) {
    let mut out: Vec<(KeysPreset, PathBuf)> = Vec::new();
    for kind in [Kind::Engine, Kind::Layer] {
        let dir = root().join(kind.dir());
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut found: Vec<(KeysPreset, PathBuf)> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "styx"))
            .filter_map(|p| {
                let entry = match kind {
                    Kind::Layer => read_layer(&p).map(|l| row(kind, l.name, &l.engine)),
                    Kind::Engine => read_engine(&p).map(|e| {
                        let tag = e.engine.name.clone();
                        row(kind, e.name, &tag)
                    }),
                };
                match entry {
                    Ok(r) => Some((r, p)),
                    Err(e) => {
                        tracing::warn!("keys rig: saved preset skipped — {e}");
                        None
                    }
                }
            })
            .collect();
        found.sort_by(|a, b| a.0.name.to_lowercase().cmp(&b.0.name.to_lowercase()));
        out.extend(found);
    }
    out.into_iter().unzip()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_safe_file_names() {
        assert_eq!(file_stem(" Warm / Wide "), "Warm - Wide");
        assert_eq!(file_stem(""), "Untitled");
    }
}
