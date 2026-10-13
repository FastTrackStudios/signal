//! Filesystem host for `InputConfigService` (input-config-proto).
//!
//! Serves a config directory (`<config_dir>/keybinds`, `.../keybinds/
//! overlays`, `.../workflows` — the same layout `ui::keyboard::source`
//! resolves) over the vox RPC trait, so remote editors (web keybind
//! editor, the site, the hub) read and write the same styx files the
//! live input processor hot-reloads. Every write snapshots first
//! (file-backed undo, same as the in-process editor).

use std::path::{Path, PathBuf};

use input_config_proto::{
    InputConfigService, OverlayConfig, OverlayInfo, ProfileConfig, ProfileInfo, SectionConfig,
    WorkflowConfig, WorkflowInfo, WriteResult, kebab_to_title,
};

use super::{editor, undo};

/// Serves one config directory tree.
#[derive(Clone)]
pub struct InputConfigHost {
    /// `<config_dir>/keybinds` — profile directories + `overlays/`.
    pub keybinds_dir: PathBuf,
    /// `<config_dir>/workflows`.
    pub workflows_dir: PathBuf,
}

impl InputConfigHost {
    /// Host the standard layout under a config dir
    /// (e.g. `~/fts-dev/fasttrackstudio/input`).
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        let config_dir = config_dir.into();
        Self {
            keybinds_dir: config_dir.join("keybinds"),
            workflows_dir: config_dir.join("workflows"),
        }
    }

    fn overlays_dir(&self) -> PathBuf {
        self.keybinds_dir.join("overlays")
    }

    fn profile_dir(&self, profile: &str) -> Option<PathBuf> {
        let dir = self.keybinds_dir.join(sanitize(profile)?);
        dir.join("profile.styx").exists().then_some(dir)
    }

    fn read_styx<T: facet::Facet<'static>>(path: &Path) -> Option<T> {
        let content = std::fs::read_to_string(path).ok()?;
        match facet_styx::from_str(&content) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(path = %path.display(), "styx parse failed: {e}");
                None
            }
        }
    }

    fn write_styx<T: facet::Facet<'static>>(path: &Path, value: &T, reason: &str) -> WriteResult {
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return WriteResult::err(format!("creating {}: {e}", parent.display()));
        }
        let content = match facet_styx::to_string(value) {
            Ok(c) => c,
            Err(e) => return WriteResult::err(format!("serialize: {e}")),
        };
        // Never persist output the parser can't read back (serializer and
        // parser have disagreed before — see the vendored styx-format fix).
        if let Err(e) = facet_styx::from_str::<T>(&content) {
            return WriteResult::err(format!("serialized config does not re-parse: {e}"));
        }
        undo::snapshot(path, reason);
        match std::fs::write(path, &content) {
            Ok(()) => WriteResult::ok(),
            Err(e) => WriteResult::err(format!("writing {}: {e}", path.display())),
        }
    }

    /// Filename-stem ids of `.styx` files directly under `dir`.
    fn styx_stems(dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut stems: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                (path.is_file() && path.extension().is_some_and(|x| x == "styx"))
                    .then(|| path.file_stem()?.to_str().map(str::to_string))
                    .flatten()
            })
            .collect();
        stems.sort();
        stems
    }
}

/// Reject ids that could escape the config tree.
fn sanitize(id: &str) -> Option<&str> {
    (!id.is_empty() && !id.contains(['/', '\\']) && id != "." && id != ".." && id != "overlays")
        .then_some(id)
}

impl InputConfigService for InputConfigHost {
    async fn list_profiles(&self) -> Vec<ProfileInfo> {
        let Ok(entries) = std::fs::read_dir(&self.keybinds_dir) else {
            return Vec::new();
        };
        let mut profiles: Vec<ProfileInfo> = entries
            .flatten()
            .filter_map(|e| {
                let dir = e.path();
                let id = dir.file_name()?.to_str()?.to_string();
                sanitize(&id)?;
                let config: ProfileConfig = Self::read_styx(&dir.join("profile.styx"))?;
                Some(ProfileInfo {
                    id,
                    name: config.name,
                    description: config.description,
                    version: config.version,
                })
            })
            .collect();
        profiles.sort_by(|a, b| a.id.cmp(&b.id));
        profiles
    }

    async fn read_profile(&self, profile: String) -> Option<ProfileConfig> {
        Self::read_styx(&self.profile_dir(&profile)?.join("profile.styx"))
    }

    async fn create_profile(&self, id: String, name: String) -> WriteResult {
        let Some(id) = sanitize(&id) else {
            return WriteResult::err("invalid profile id");
        };
        match editor::create_profile(&self.keybinds_dir, id, &name) {
            Ok(_) => WriteResult::ok(),
            Err(e) => WriteResult::err(format!("{e:?}")),
        }
    }

    async fn read_section(&self, profile: String, section: String) -> Option<SectionConfig> {
        let dir = self.profile_dir(&profile)?;
        let section = sanitize(section.trim_end_matches(".styx"))?;
        Self::read_styx(&dir.join(format!("{section}.styx")))
    }

    async fn write_section(
        &self,
        profile: String,
        section: String,
        config: SectionConfig,
    ) -> WriteResult {
        let Some(dir) = self.profile_dir(&profile) else {
            return WriteResult::err(format!("unknown profile: {profile}"));
        };
        let Some(section) = sanitize(section.trim_end_matches(".styx")) else {
            return WriteResult::err("invalid section name");
        };
        Self::write_styx(
            &dir.join(format!("{section}.styx")),
            &config,
            "rpc section write",
        )
    }

    async fn list_overlays(&self) -> Vec<OverlayInfo> {
        let dir = self.overlays_dir();
        Self::styx_stems(&dir)
            .into_iter()
            .filter_map(|id| {
                let config: OverlayConfig = Self::read_styx(&dir.join(format!("{id}.styx")))?;
                Some(OverlayInfo {
                    id,
                    name: config.name,
                    description: config.description,
                    priority: config.priority,
                })
            })
            .collect()
    }

    async fn read_overlay(&self, id: String) -> Option<OverlayConfig> {
        let id = sanitize(&id)?;
        Self::read_styx(&self.overlays_dir().join(format!("{id}.styx")))
    }

    async fn write_overlay(&self, id: String, config: OverlayConfig) -> WriteResult {
        let Some(id) = sanitize(&id) else {
            return WriteResult::err("invalid overlay id");
        };
        Self::write_styx(
            &self.overlays_dir().join(format!("{id}.styx")),
            &config,
            "rpc overlay write",
        )
    }

    async fn list_workflows(&self) -> Vec<WorkflowInfo> {
        Self::styx_stems(&self.workflows_dir)
            .into_iter()
            .filter_map(|id| {
                let config: WorkflowConfig =
                    Self::read_styx(&self.workflows_dir.join(format!("{id}.styx")))?;
                Some(WorkflowInfo {
                    name: config.name.clone().unwrap_or_else(|| kebab_to_title(&id)),
                    description: config.description.clone().unwrap_or_default(),
                    id,
                })
            })
            .collect()
    }

    async fn read_workflow(&self, id: String) -> Option<WorkflowConfig> {
        let id = sanitize(&id)?;
        Self::read_styx(&self.workflows_dir.join(format!("{id}.styx")))
    }

    async fn write_workflow(&self, id: String, config: WorkflowConfig) -> WriteResult {
        let Some(id) = sanitize(&id) else {
            return WriteResult::err("invalid workflow id");
        };
        Self::write_styx(
            &self.workflows_dir.join(format!("{id}.styx")),
            &config,
            "rpc workflow write",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use input_config_proto::KeybindDef;

    fn temp_config_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fts-input-config-host-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("keybinds/overlays")).unwrap();
        std::fs::create_dir_all(dir.join("workflows")).unwrap();
        dir
    }

    #[tokio::test]
    async fn profile_section_round_trip() {
        let dir = temp_config_dir();
        let host = InputConfigHost::new(&dir);

        assert!(host.create_profile("test".into(), "Test".into()).await.ok);
        let profiles = host.list_profiles().await;
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, "test");

        let mut section = host
            .read_section("test".into(), "bindings".into())
            .await
            .expect("scaffolded section");
        section.bindings = Some(vec![KeybindDef {
            keys: "<C-s>".into(),
            action: "40026".into(),
            desc: Some("Save".into()),
            context: None,
            passthrough: None,
            mnemonic: None,
            why: None,
        }]);
        assert!(
            host.write_section("test".into(), "bindings".into(), section)
                .await
                .ok
        );

        let read_back = host
            .read_section("test".into(), "bindings".into())
            .await
            .unwrap();
        assert_eq!(read_back.bindings().len(), 1);
        assert_eq!(read_back.bindings()[0].keys, "<C-s>");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn workflow_round_trip_and_traversal_rejected() {
        let dir = temp_config_dir();
        let host = InputConfigHost::new(&dir);

        let wf = WorkflowConfig {
            keybind_overlays: Some(vec!["quick-edit".into()]),
            ..Default::default()
        };
        assert!(host.write_workflow("mode-test".into(), wf).await.ok);
        let listed = host.list_workflows().await;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Mode Test");

        assert!(
            !host
                .write_workflow("../escape".into(), WorkflowConfig::default())
                .await
                .ok
        );
        assert!(host.read_section("../x".into(), "y".into()).await.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
