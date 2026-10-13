//! Toolbar — Dynamic toolbar button management
//!
//! Client-side handle for adding, updating, and removing toolbar buttons
//! in the host DAW.

use crate::DawClients;
use std::sync::Arc;

/// Handle for managing toolbar buttons in the host DAW.
pub struct Toolbar {
    clients: Arc<DawClients>,
}

impl Toolbar {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    /// Add a toolbar button. Returns the resolved command ID on success.
    pub async fn add_button(
        &self,
        button: daw_proto::ToolbarButton,
        workflow_id: &str,
    ) -> crate::Result<daw_proto::ToolbarResult> {
        Ok(self
            .clients
            .toolbar
            .add_button(button, workflow_id.to_string())
            .await?)
    }

    /// Update a toolbar button (or add if not present).
    pub async fn update_button(
        &self,
        button: daw_proto::ToolbarButton,
        workflow_id: &str,
    ) -> crate::Result<daw_proto::ToolbarResult> {
        Ok(self
            .clients
            .toolbar
            .update_button(button, workflow_id.to_string())
            .await?)
    }

    /// Remove a single toolbar button.
    pub async fn remove_button(
        &self,
        target: daw_proto::ToolbarTarget,
        command_name: &str,
    ) -> crate::Result<daw_proto::ToolbarResult> {
        Ok(self
            .clients
            .toolbar
            .remove_button(target, command_name.to_string())
            .await?)
    }

    /// Move a toolbar button to a zero-based position.
    pub async fn move_button(
        &self,
        target: daw_proto::ToolbarTarget,
        command_name: &str,
        position: u32,
    ) -> crate::Result<daw_proto::ToolbarResult> {
        Ok(self
            .clients
            .toolbar
            .move_button(target, command_name.to_string(), position)
            .await?)
    }

    /// Set or clear a toolbar button icon.
    pub async fn set_button_icon(
        &self,
        target: daw_proto::ToolbarTarget,
        command_name: &str,
        icon: Option<daw_proto::ToolbarIcon>,
    ) -> crate::Result<daw_proto::ToolbarResult> {
        Ok(self
            .clients
            .toolbar
            .set_button_icon(target, command_name.to_string(), icon)
            .await?)
    }

    /// Remove all toolbar buttons belonging to a workflow.
    pub async fn remove_workflow_buttons(
        &self,
        workflow_id: &str,
    ) -> crate::Result<daw_proto::ToolbarResult> {
        Ok(self
            .clients
            .toolbar
            .remove_workflow_buttons(workflow_id.to_string())
            .await?)
    }

    /// Check if the dynamic toolbar API is available.
    pub async fn is_available(&self) -> crate::Result<bool> {
        Ok(self.clients.toolbar.is_available().await?)
    }

    /// List all tracked buttons.
    pub async fn get_tracked_buttons(&self) -> crate::Result<Vec<daw_proto::TrackedButton>> {
        Ok(self.clients.toolbar.tracked_buttons().await?)
    }

    /// Snapshot one live toolbar as JSON.
    pub async fn get_live_toolbar_json(
        &self,
        target: daw_proto::ToolbarTarget,
    ) -> crate::Result<String> {
        let target_name = match target {
            daw_proto::ToolbarTarget::Main => "Main toolbar".to_string(),
            daw_proto::ToolbarTarget::Floating(n) => format!("Floating toolbar {n}"),
            daw_proto::ToolbarTarget::Midi(n) => format!("Floating MIDI toolbar {n}"),
        };
        live_toolbar_rows_json(
            self.clients
                .toolbar
                .tracked_buttons()
                .await?
                .into_iter()
                .filter(|row| row.toolbar_name == target_name),
        )
    }

    /// Snapshot all non-empty live toolbars as JSON.
    pub async fn get_live_toolbars_json(&self) -> crate::Result<String> {
        live_toolbar_rows_json(self.clients.toolbar.tracked_buttons().await?.into_iter())
    }

    /// Parse all toolbar sections from a REAPER menu/toolbar config file as JSON.
    pub async fn parse_toolbar_config_json(&self, path: &str) -> crate::Result<String> {
        Err(crate::Error::Other(format!(
            "toolbar config parsing is implemented by daw-cli for local path: {path}"
        )))
    }
}

fn live_toolbar_rows_json(
    rows: impl Iterator<Item = daw_proto::TrackedButton>,
) -> crate::Result<String> {
    let mut toolbars = std::collections::BTreeMap::<String, Vec<daw_proto::ToolbarItemInfo>>::new();
    for row in rows.filter(|row| row.workflow_id == "__fts_live_toolbar_item") {
        let item = facet_json::from_str::<daw_proto::ToolbarItemInfo>(&row.command_name)
            .map_err(|err| crate::Error::Other(format!("decode live toolbar row: {err}")))?;
        toolbars.entry(row.toolbar_name).or_default().push(item);
    }

    let snapshots = toolbars
        .into_iter()
        .map(|(toolbar_name, items)| daw_proto::ToolbarSnapshot {
            toolbar_name,
            source: daw_proto::ToolbarSnapshotSource::Live,
            items,
        })
        .collect::<Vec<_>>();
    facet_json::to_string(&snapshots)
        .map_err(|err| crate::Error::Other(format!("encode live toolbar snapshot: {err}")))
}
