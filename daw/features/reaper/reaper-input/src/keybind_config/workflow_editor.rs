//! Workflow config creation and editing.
//!
//! Creates new workflow .styx files and their associated overlay files.

use std::path::PathBuf;

use crate::keybind_config::types::{KeybindDef, OverlayConfig, ReaperSettingDef, WorkflowConfig};

/// Create a new workflow with its associated overlay files.
pub struct WorkflowCreator {
    /// Workflow directory (e.g., ~/fts-dev/fasttrackstudio/input/workflows/)
    workflow_dir: PathBuf,
    /// Overlay directory (e.g., ~/fts-dev/fasttrackstudio/input/keybinds/overlays/)
    overlay_dir: PathBuf,
}

/// Parameters for creating a new workflow.
#[derive(Clone, Debug, Default)]
pub struct NewWorkflow {
    /// kebab-case ID (e.g., "my-workflow"). Becomes the filename.
    pub id: String,
    /// Optional display name override. If empty, derived from ID.
    pub name: Option<String>,
    /// Description of what the workflow does.
    pub description: String,
    /// REAPER settings to toggle when workflow activates.
    pub settings: Vec<NewReaperSetting>,
    /// Initial keybindings for the workflow's overlay.
    pub bindings: Vec<NewBinding>,
}

#[derive(Clone, Debug)]
pub struct NewReaperSetting {
    pub command: String,
    pub enabled: bool,
    pub desc: String,
}

#[derive(Clone, Debug)]
pub struct NewBinding {
    pub keys: String,
    pub action: String,
    pub desc: String,
}

impl WorkflowCreator {
    pub fn new(workflow_dir: impl Into<PathBuf>, overlay_dir: impl Into<PathBuf>) -> Self {
        Self {
            workflow_dir: workflow_dir.into(),
            overlay_dir: overlay_dir.into(),
        }
    }

    /// Create a new workflow and its overlay files.
    pub fn create(&self, workflow: &NewWorkflow) -> Result<CreatedFiles, String> {
        std::fs::create_dir_all(&self.workflow_dir)
            .map_err(|e| format!("Failed to create workflow dir: {e}"))?;
        std::fs::create_dir_all(&self.overlay_dir)
            .map_err(|e| format!("Failed to create overlay dir: {e}"))?;

        let workflow_path = self.workflow_dir.join(format!("{}.styx", workflow.id));
        let overlay_path = self.overlay_dir.join(format!("{}.styx", workflow.id));

        // Check for conflicts
        if workflow_path.exists() {
            return Err(format!("Workflow '{}' already exists", workflow.id));
        }

        // Build workflow config
        let workflow_config = WorkflowConfig {
            name: workflow.name.clone(),
            description: if workflow.description.is_empty() {
                None
            } else {
                Some(workflow.description.clone())
            },
            keybind_overlays: Some(vec![workflow.id.clone()]),
            mouse_overlays: None,
            settings: if workflow.settings.is_empty() {
                None
            } else {
                Some(
                    workflow
                        .settings
                        .iter()
                        .map(|s| ReaperSettingDef {
                            command: s.command.clone(),
                            enabled: s.enabled,
                            desc: Some(s.desc.clone()),
                        })
                        .collect(),
                )
            },
            bindings: None,
            wheel: None,
            mouse_settings: None,
            armed_action: None,
        };

        // Build overlay config
        let title = crate::keybind_config::types::kebab_to_title(&workflow.id);
        let display_name = workflow.name.as_deref().unwrap_or(&title);
        let overlay_config = OverlayConfig {
            name: workflow.id.clone(),
            description: workflow.description.clone(),
            priority: 50,
            bindings: if workflow.bindings.is_empty() {
                None
            } else {
                Some(
                    workflow
                        .bindings
                        .iter()
                        .map(|b| KeybindDef {
                            keys: b.keys.clone(),
                            action: b.action.clone(),
                            desc: if b.desc.is_empty() {
                                None
                            } else {
                                Some(b.desc.clone())
                            },
                            context: None,
                            passthrough: None,
                            mnemonic: None,
                            why: None,
                        })
                        .collect(),
                )
            },
            wheel: None,
            mouse: None,
            mouse_settings: None,
        };

        // Serialize and write
        let workflow_styx = facet_styx::to_string(&workflow_config)
            .map_err(|e| format!("Failed to serialize workflow: {e}"))?;
        let overlay_styx = facet_styx::to_string(&overlay_config)
            .map_err(|e| format!("Failed to serialize overlay: {e}"))?;

        std::fs::write(&workflow_path, &workflow_styx)
            .map_err(|e| format!("Failed to write workflow file: {e}"))?;
        std::fs::write(&overlay_path, &overlay_styx)
            .map_err(|e| format!("Failed to write overlay file: {e}"))?;

        tracing::info!(
            id = workflow.id.as_str(),
            "Created workflow '{}' → {} + {}",
            display_name,
            workflow_path.display(),
            overlay_path.display(),
        );

        Ok(CreatedFiles {
            workflow_path,
            overlay_path,
        })
    }

    /// List existing workflow IDs from the workflow directory.
    pub fn list_workflows(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.workflow_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "styx")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    ids.push(stem.to_string());
                }
            }
        }
        ids.sort();
        ids
    }
}

pub struct CreatedFiles {
    pub workflow_path: PathBuf,
    pub overlay_path: PathBuf,
}
