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
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry seeded with the built-in **code** presets (Nord Stage, the
    /// layering demo, …).
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

    pub fn get(&self, name: &str) -> Option<&RegisteredPreset> {
        self.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// The composition tree for `name`, if registered.
    pub fn tree(&self, name: &str) -> Option<&Container> {
        self.get(name).map(|p| &p.tree)
    }

    pub fn names(&self) -> Vec<&str> {
        self.presets.iter().map(|p| p.name.as_str()).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &RegisteredPreset> {
        self.presets.iter()
    }

    pub fn len(&self) -> usize {
        self.presets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig_node::Role;
    use signal_proto::block::BlockType;

    #[test]
    fn builtins_register_nord_from_code() {
        let reg = PresetRegistry::with_builtins();
        let nord = reg.get("Nord Stage").expect("Nord Stage builtin");
        assert_eq!(nord.source, PresetSource::Code);
        assert_eq!(nord.tree.of_role(Role::Layer).len(), 7);
    }

    #[test]
    fn register_a_code_preset() {
        let mut reg = PresetRegistry::new();
        reg.register_code(
            Container::preset("My Test")
                .add(Container::layer("A").block(BlockType::Oscillator, "Osc")),
        );
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get("my test").unwrap().source, PresetSource::Code);
        assert_eq!(reg.tree("My Test").unwrap().blocks().len(), 1);
    }

    #[test]
    fn load_a_styx_preset_from_disk() {
        // Serialize a code tree to a temp .styx, then load it back through the
        // registry — proving the styx registration path end to end.
        let tree = Container::preset("Disk Preset")
            .add(Container::layer("A").block(BlockType::Sampler, "Piano"));
        let dir = std::env::temp_dir().join(format!("signal-preset-reg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("disk-preset.styx");
        std::fs::write(&path, tree.to_styx_string().unwrap()).unwrap();

        let mut reg = PresetRegistry::new();
        reg.load_styx_dir(&dir);
        assert!(reg.errors.is_empty(), "{:?}", reg.errors);
        let loaded = reg.get("Disk Preset").expect("loaded from styx");
        assert_eq!(loaded.source, PresetSource::Styx(path));
        assert_eq!(loaded.tree.blocks().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn code_and_styx_presets_coexist_and_styx_overrides() {
        let mut reg = PresetRegistry::with_builtins(); // Nord Stage (code)
        // A styx file named "Nord Stage" overrides the built-in (last wins).
        let dir = std::env::temp_dir().join(format!("signal-preg2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let override_tree = Container::preset("Nord Stage")
            .add(Container::layer("Solo").block(BlockType::Oscillator, "Osc"));
        std::fs::write(
            dir.join("nord.styx"),
            override_tree.to_styx_string().unwrap(),
        )
        .unwrap();
        let before = reg.len();
        reg.load_styx_dir(&dir);

        // Same count (override, not add); "Nord Stage" now sourced from styx.
        assert_eq!(reg.len(), before);
        let nord = reg.get("Nord Stage").unwrap();
        assert!(matches!(nord.source, PresetSource::Styx(_)));
        assert_eq!(nord.tree.of_role(Role::Layer).len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
