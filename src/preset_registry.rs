//! Registry of composition-tree presets ([`Container`] trees), registered from
//! **code** (traceable Rust factories) *and* loaded from **`.styx`** files.
//!
//! Code presets keep test rigs fully traceable (you can read the factory); styx
//! presets let content live on disk in the library. Both land in the same
//! registry, keyed by name (last registration wins, so a styx file can override
//! a built-in).

use std::path::{Path, PathBuf};

use crate::rig_node::Container;

/// Where a registered preset came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetSource {
    /// Built in code by a Rust factory — fully traceable.
    Code,
    /// Loaded from a `.styx` file at this path.
    Styx(PathBuf),
}

/// A named preset plus its provenance and the composition tree.
#[derive(Debug, Clone)]
pub struct RegisteredPreset {
    pub name: String,
    pub source: PresetSource,
    pub tree: Container,
}

/// An in-memory set of presets from code + styx.
#[derive(Debug, Clone, Default)]
pub struct PresetRegistry {
    presets: Vec<RegisteredPreset>,
    /// Non-fatal styx load errors (`<file>: <message>`).
    pub errors: Vec<String>,
}

impl PresetRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry seeded with the built-in **code** presets (Nord Stage, the
    /// layering demo, …).
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.register_code(crate::nord::nord_stage_preset());
        // Sample-realized variant — only on machines with the Keyscape
        // extraction (nord_stage_piano_preset probes the library paths).
        if let Some(piano) = crate::nord::nord_stage_piano_preset() {
            r.register_code(piano);
        }
        r.register_code(crate::nord::layering_demo());
        r.register_code(crate::nord::city_grand_preset());
        r.register_code(crate::nord::city_wurli_preset());
        r
    }

    /// Register a code-built preset; its name is taken from the tree's root.
    pub fn register_code(&mut self, tree: Container) {
        let name = tree.name.clone();
        self.insert(RegisteredPreset {
            name,
            source: PresetSource::Code,
            tree,
        });
    }

    /// Load every `*.styx` file directly under `dir` as a preset. Missing dir is
    /// fine; per-file parse errors are collected into [`errors`](Self::errors).
    pub fn load_styx_dir(&mut self, dir: impl AsRef<Path>) {
        if let Ok(rd) = std::fs::read_dir(dir.as_ref()) {
            let mut paths: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("styx"))
                .collect();
            paths.sort();
            for p in paths {
                self.load_styx_file(&p);
            }
        }
    }

    /// Load a single `.styx` preset file.
    pub fn load_styx_file(&mut self, path: &Path) {
        match Container::from_styx_file(path) {
            Ok(tree) => {
                let name = tree.name.clone();
                self.insert(RegisteredPreset {
                    name,
                    source: PresetSource::Styx(path.to_path_buf()),
                    tree,
                });
            }
            Err(e) => self.errors.push(format!("{}: {e}", path.display())),
        }
    }

    /// Insert, replacing any existing preset of the same name (last wins).
    fn insert(&mut self, preset: RegisteredPreset) {
        if let Some(slot) = self
            .presets
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&preset.name))
        {
            *slot = preset;
        } else {
            self.presets.push(preset);
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&RegisteredPreset> {
        self.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// The composition tree for `name`, if registered.
    #[must_use]
    pub fn tree(&self, name: &str) -> Option<&Container> {
        self.get(name).map(|p| &p.tree)
    }

    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.presets.iter().map(|p| p.name.as_str()).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &RegisteredPreset> {
        self.presets.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.presets.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }
}
