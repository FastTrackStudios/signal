//! Workflow System
//!
//! Workflows are high-level context switches that coordinate:
//! - Keybind overlays
//! - Mouse modifier overlays
//! - REAPER settings (toggles, preferences)
//! - Armed click actions (custom click interception)
//!
//! Examples: Tempo Mapping, Fast Slip Edit, Recording, Mixing
//!
//! ## Profile-Based Implementations
//!
//! Workflows are **profile-agnostic** - they represent a concept (e.g., "Tempo Mapping")
//! rather than a specific implementation. Each profile (FastTrackStudio, Logic, etc.)
//! can register its own implementation of a workflow, or use the default.
//!
//! When a workflow is activated:
//! 1. Check if the current profile has a registered implementation
//! 2. If not, use the default implementation
//! 3. If no implementation exists, return an error

pub mod armed;
pub mod context_detection;
pub mod slip_drag;

use crate::infrastructure::toolbar::{self, ToolbarButton, ToolbarTarget};
use crate::input::keybinds;
use crate::input::mouse_modifiers::manager::{self as mouse_manager, MouseSnapshot};
pub use armed::{
    ArmedClickAction,
    ArmedContext,
    ItemHitInfo,
    MouseContextResult,
    // Comprehensive MM_CTX detection
    MouseModifierContext,
    detect_context_at_point,
    detect_mouse_modifier_context,
    is_debug_mouse_context_enabled,
    toggle_debug_mouse_context,
};
use reaper_high::Reaper;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};
use tracing::{debug, info, warn};

// region: --- Workflow Definition (Profile-Agnostic)

/// A workflow definition - the profile-agnostic concept
///
/// This represents "what" a workflow is (e.g., "Tempo Mapping"),
/// not "how" it's implemented for a specific profile.
#[derive(Debug, Clone)]
pub struct WorkflowDefinition {
    /// Unique workflow ID (e.g., "tempo_mapping")
    pub id: &'static str,
    /// Human-readable name (e.g., "Tempo Mapping")
    pub name: &'static str,
    /// Short name for toolbar display (e.g., "Tempo")
    pub short_name: Option<&'static str>,
    /// Description of what this workflow does
    pub description: &'static str,
}

impl WorkflowDefinition {
    pub fn new(id: &'static str, name: &'static str, description: &'static str) -> Self {
        Self {
            id,
            name,
            short_name: None,
            description,
        }
    }

    pub fn with_short_name(
        id: &'static str,
        name: &'static str,
        short_name: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            id,
            name,
            short_name: Some(short_name),
            description,
        }
    }

    /// Get the display name for toolbar buttons
    ///
    /// Uses short_name if:
    /// - The name is longer than 12 characters AND has no spaces
    /// - A short_name is defined
    ///
    /// Otherwise uses the full name.
    pub fn display_name(&self) -> &str {
        if let Some(short) = self.short_name {
            // Use short name if full name is > 12 chars with no spaces
            if self.name.len() > 12 && !self.name.contains(' ') {
                return short;
            }
        }
        self.name
    }
}

// endregion: --- Workflow Definition

// region: --- Workflow Implementation (Profile-Specific)

/// A workflow implementation - the profile-specific settings
///
/// This represents "how" a workflow is implemented for a specific profile,
/// including keybinds, mouse modifiers, REAPER settings, etc.
#[derive(Debug, Clone, Default)]
pub struct WorkflowImplementation {
    /// Keybind overlays to enable
    pub keybind_overlays: Vec<String>,
    /// Mouse modifier overlays to enable
    pub mouse_overlays: Vec<String>,
    /// REAPER settings to apply
    pub reaper_settings: Vec<ReaperSetting>,
    /// Action to arm when workflow is active (uses REAPER's native arm)
    pub armed_action: Option<ArmedAction>,
    /// Custom armed click action (intercepts left-clicks)
    pub armed_click: Option<ArmedClickAction>,
    /// Toolbar buttons to add when workflow is active
    pub toolbar_buttons: Vec<ToolbarButton>,
}

impl WorkflowImplementation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a keybind overlay to enable
    pub fn with_keybind_overlay(mut self, overlay: impl Into<String>) -> Self {
        self.keybind_overlays.push(overlay.into());
        self
    }

    /// Add multiple keybind overlays
    pub fn with_keybind_overlays(
        mut self,
        overlays: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.keybind_overlays
            .extend(overlays.into_iter().map(Into::into));
        self
    }

    /// Add a mouse modifier overlay to enable
    pub fn with_mouse_overlay(mut self, overlay: impl Into<String>) -> Self {
        self.mouse_overlays.push(overlay.into());
        self
    }

    /// Add multiple mouse modifier overlays
    pub fn with_mouse_overlays(
        mut self,
        overlays: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.mouse_overlays
            .extend(overlays.into_iter().map(Into::into));
        self
    }

    /// Add a REAPER setting
    pub fn with_setting(mut self, setting: ReaperSetting) -> Self {
        self.reaper_settings.push(setting);
        self
    }

    /// Add multiple REAPER settings
    pub fn with_settings(mut self, settings: impl IntoIterator<Item = ReaperSetting>) -> Self {
        self.reaper_settings.extend(settings);
        self
    }

    /// Set an action to arm when workflow is active
    pub fn with_armed_action(mut self, action: ArmedAction) -> Self {
        self.armed_action = Some(action);
        self
    }

    /// Set a custom armed click action
    pub fn with_armed_click(mut self, action: ArmedClickAction) -> Self {
        self.armed_click = Some(action);
        self
    }

    /// Add a toolbar button to a floating toolbar
    pub fn with_floating_toolbar_button(
        mut self,
        toolbar_num: u8,
        command_name: &str,
        label: &str,
    ) -> Self {
        self.toolbar_buttons.push(ToolbarButton {
            command_name: command_name.to_string(),
            label: label.to_string(),
            icon: None,
            toolbar: ToolbarTarget::Floating(toolbar_num),
            toolbar_flags: toolbar::flags::NORMAL,
        });
        self
    }

    /// Add a toolbar button to the main toolbar
    pub fn with_main_toolbar_button(mut self, command_name: &str, label: &str) -> Self {
        self.toolbar_buttons.push(ToolbarButton {
            command_name: command_name.to_string(),
            label: label.to_string(),
            icon: None,
            toolbar: ToolbarTarget::Main,
            toolbar_flags: toolbar::flags::NORMAL,
        });
        self
    }
}

// endregion: --- Workflow Implementation

// region: --- REAPER Settings

/// A REAPER setting that can be toggled
#[derive(Debug, Clone)]
pub struct ReaperSetting {
    /// Human-readable name
    pub name: String,
    /// Command ID to toggle (or action name)
    pub command: String,
    /// Desired state when workflow is active
    pub enabled: bool,
}

/// An action to arm when the workflow is active
#[derive(Debug, Clone)]
pub struct ArmedAction {
    /// Human-readable name
    pub name: String,
    /// Command ID or action name to arm
    pub command: String,
    /// Section name (empty string for main section)
    pub section: String,
    /// If true, intercept left-clicks to trigger this action
    /// If false, only REAPER's native arm mechanism is used (may not work for all actions)
    pub intercept_clicks: bool,
}

impl ArmedAction {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            section: String::new(),  // Main section by default
            intercept_clicks: false, // Default to not intercepting clicks
        }
    }

    pub fn with_section(mut self, section: impl Into<String>) -> Self {
        self.section = section.into();
        self
    }

    /// Enable click interception - left-clicks in arrange will trigger this action
    pub fn with_click_intercept(mut self) -> Self {
        self.intercept_clicks = true;
        self
    }
}

impl ReaperSetting {
    pub fn new(name: impl Into<String>, command: impl Into<String>, enabled: bool) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            enabled,
        }
    }

    /// Create a setting that should be ON when workflow is active
    pub fn on(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self::new(name, command, true)
    }

    /// Create a setting that should be OFF when workflow is active
    pub fn off(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self::new(name, command, false)
    }
}

/// Stored state for restoration when workflow is deactivated
#[derive(Debug, Clone, Default)]
struct StoredState {
    /// Keybind overlays that were active before workflow
    keybind_overlays: Vec<String>,
    /// Mouse overlays that were active before workflow
    mouse_overlays: Vec<String>,
    /// Original toggle states for REAPER settings (command -> was_on)
    reaper_settings: HashMap<String, bool>,
    /// Previously armed command (cmd_id, section) - 0 if nothing was armed
    previous_armed: Option<(i32, String)>,
    /// Full snapshot of REAPER mouse modifier state taken just before activation.
    /// Restored on deactivation so all mouse modifier settings return to their
    /// pre-workflow values, even if overlays don't track every context.
    mouse_snapshot: Option<MouseSnapshot>,
}

// endregion: --- Stored State

// region: --- Workflow Manager

/// Manages workflows and their activation state
///
/// Workflows are stored as:
/// - **Definitions**: Profile-agnostic workflow concepts (id, name, description)
/// - **Default implementations**: The fallback implementation for each workflow
/// - **Profile implementations**: Profile-specific overrides for workflows
pub struct WorkflowManager {
    /// Workflow definitions (id -> definition)
    definitions: HashMap<String, WorkflowDefinition>,
    /// Default implementations (workflow_id -> implementation)
    default_implementations: HashMap<String, WorkflowImplementation>,
    /// Profile-specific implementations ((workflow_id, profile_id) -> implementation)
    profile_implementations: HashMap<(String, String), WorkflowImplementation>,
    /// Currently active workflow ID (only one at a time)
    active_workflow: Option<String>,
    /// The resolved implementation currently in use
    active_implementation: Option<WorkflowImplementation>,
    /// Stored state for restoration
    stored_state: StoredState,
}

impl WorkflowManager {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
            default_implementations: HashMap::new(),
            profile_implementations: HashMap::new(),
            active_workflow: None,
            active_implementation: None,
            stored_state: StoredState::default(),
        }
    }

    // region: --- New API (Profile-Based)

    /// Define a workflow (profile-agnostic)
    ///
    /// This creates the workflow concept without any implementation.
    /// Use `implement()` or `implement_for_profile()` to add implementations.
    pub fn define(&mut self, id: &'static str, name: &'static str, description: &'static str) {
        debug!(id = %id, name = %name, "Defining workflow");
        self.definitions.insert(
            id.to_string(),
            WorkflowDefinition::new(id, name, description),
        );
    }

    /// Define a workflow with a short name for toolbar display
    ///
    /// The short name is used when the full name is > 12 chars with no spaces.
    pub fn define_with_short_name(
        &mut self,
        id: &'static str,
        name: &'static str,
        short_name: &'static str,
        description: &'static str,
    ) {
        debug!(id = %id, name = %name, short_name = %short_name, "Defining workflow with short name");
        self.definitions.insert(
            id.to_string(),
            WorkflowDefinition::with_short_name(id, name, short_name, description),
        );
    }

    /// Register the default implementation for a workflow
    ///
    /// This is used when no profile-specific implementation is available.
    pub fn implement(&mut self, workflow_id: &str, implementation: WorkflowImplementation) {
        debug!(workflow_id = %workflow_id, "Registering default implementation");
        self.default_implementations
            .insert(workflow_id.to_string(), implementation);
    }

    /// Register a profile-specific implementation for a workflow
    ///
    /// This overrides the default implementation for the specified profile.
    pub fn implement_for_profile(
        &mut self,
        workflow_id: &str,
        profile: &str,
        implementation: WorkflowImplementation,
    ) {
        debug!(workflow_id = %workflow_id, profile = %profile, "Registering profile implementation");
        self.profile_implementations.insert(
            (workflow_id.to_string(), profile.to_string()),
            implementation,
        );
    }

    /// Get the implementation for a workflow based on the current profile
    ///
    /// Returns the profile-specific implementation if available, otherwise the default.
    pub fn get_implementation(
        &self,
        workflow_id: &str,
        profile: &str,
    ) -> Option<&WorkflowImplementation> {
        // Try profile-specific first
        let key = (workflow_id.to_string(), profile.to_string());
        if let Some(impl_) = self.profile_implementations.get(&key) {
            return Some(impl_);
        }

        // Fall back to default
        self.default_implementations.get(workflow_id)
    }

    /// Get all workflow definitions (profile-agnostic list)
    pub fn list_definitions(&self) -> Vec<(&str, &str, &str)> {
        self.definitions
            .values()
            .map(|d| (d.id, d.name, d.description))
            .collect()
    }

    /// Get a workflow definition by ID
    pub fn get_definition(&self, id: &str) -> Option<&WorkflowDefinition> {
        self.definitions.get(id)
    }

    /// Get the active workflow definition (if any)
    pub fn active_definition(&self) -> Option<&WorkflowDefinition> {
        self.active_workflow
            .as_ref()
            .and_then(|id| self.definitions.get(id))
    }

    /// List all registered workflows as (id, name, description) tuples
    pub fn list(&self) -> Vec<(&str, &str, &str)> {
        self.list_definitions()
    }

    /// Check if a workflow is active
    pub fn is_active(&self, id: &str) -> bool {
        self.active_workflow.as_deref() == Some(id)
    }

    // endregion: --- New API

    /// Activate a workflow
    ///
    /// This will:
    /// 1. Deactivate any currently active workflow
    /// 2. Look up the implementation for the current profile
    /// 3. Store current state for restoration
    /// 4. Enable all overlays for the new workflow
    /// 5. Apply REAPER settings
    pub fn activate(&mut self, id: &str) -> Result<(), String> {
        // Get workflow definition
        let definition = self
            .definitions
            .get(id)
            .ok_or_else(|| format!("Unknown workflow: {}", id))?;
        let name = definition.name;

        // Get current profile
        let current_profile = keybinds::active_preset_name();

        // Look up implementation for current profile
        let implementation = self
            .get_implementation(id, &current_profile)
            .ok_or_else(|| {
                format!(
                    "No implementation for workflow '{}' with profile '{}'",
                    id, current_profile
                )
            })?
            .clone();

        // Deactivate current workflow first (if any)
        if self.active_workflow.is_some() {
            self.deactivate();
        }

        info!(
            id = %id,
            name = %name,
            profile = %current_profile,
            "Activating workflow"
        );

        // Store current state
        self.store_current_state(&implementation);

        // Enable keybind overlays
        for overlay in &implementation.keybind_overlays {
            if !keybinds::is_override_active(overlay) {
                keybinds::enable_override(overlay);
                debug!(overlay = %overlay, "Enabled keybind overlay");
            }
        }

        // Enable mouse modifier overlays
        for overlay in &implementation.mouse_overlays {
            if !mouse_manager::is_override_active(overlay) {
                mouse_manager::enable_override(overlay);
                debug!(overlay = %overlay, "Enabled mouse modifier overlay");
            }
        }

        // Apply REAPER settings
        self.apply_reaper_settings(&implementation.reaper_settings);

        // Arm the action if specified
        info!(
            id = %id,
            has_armed_action = implementation.armed_action.is_some(),
            "activate: armed_action presence"
        );
        if let Some(ref armed) = implementation.armed_action {
            self.arm_action(armed);
        }

        // Add toolbar buttons
        info!(
            toolbar_api_available = toolbar::is_available(),
            button_count = implementation.toolbar_buttons.len(),
            "Adding toolbar buttons"
        );
        for button in &implementation.toolbar_buttons {
            match toolbar::add_button(button, id) {
                Ok(cmd_id) => {
                    info!(
                        button = %button.command_name,
                        label = %button.label,
                        toolbar = ?button.toolbar,
                        command_id = ?cmd_id,
                        "Added toolbar button"
                    );
                }
                Err(e) => {
                    warn!(
                        button = %button.command_name,
                        error = %e,
                        "Failed to add toolbar button"
                    );
                }
            }
        }

        // Store the active implementation for deactivation
        self.active_implementation = Some(implementation);
        self.active_workflow = Some(id.to_string());

        crate::trace_console_msg(format!("Workflow activated: {}\n", name));

        info!(id = %id, "Workflow activated successfully");
        Ok(())
    }

    /// Deactivate the current workflow and restore previous state
    pub fn deactivate(&mut self) -> bool {
        let Some(id) = self.active_workflow.take() else {
            return false;
        };

        // Use the stored implementation (preferred) or fall back to definition
        let implementation = match self.active_implementation.take() {
            Some(impl_) => impl_,
            None => {
                warn!(id = %id, "No stored implementation, cannot fully deactivate");
                return false;
            }
        };

        // Get the workflow name from definition
        let name = self
            .definitions
            .get(&id)
            .map(|d| d.name)
            .unwrap_or("Unknown");

        info!(id = %id, name = %name, "Deactivating workflow");

        // Remove toolbar buttons added by this workflow
        if let Err(e) = toolbar::remove_workflow_buttons(&id) {
            warn!(
                workflow = %id,
                error = %e,
                "Failed to remove toolbar buttons"
            );
        }

        // Disable keybind overlays that the workflow enabled
        for overlay in &implementation.keybind_overlays {
            // Only disable if it wasn't active before
            if !self
                .stored_state
                .keybind_overlays
                .contains(&overlay.to_string())
            {
                keybinds::disable_override(overlay);
                debug!(overlay = %overlay, "Disabled keybind overlay");
            }
        }

        // Disable mouse modifier overlays that the workflow enabled
        for overlay in &implementation.mouse_overlays {
            if !self
                .stored_state
                .mouse_overlays
                .contains(&overlay.to_string())
            {
                mouse_manager::disable_override(overlay);
                debug!(overlay = %overlay, "Disabled mouse modifier overlay");
            }
        }

        // Restore REAPER settings
        self.restore_reaper_settings(&implementation.reaper_settings);

        // Restore the full mouse modifier snapshot taken at activation time.
        // This ensures all modified contexts return to their pre-workflow values.
        if let Some(ref snapshot) = self.stored_state.mouse_snapshot {
            mouse_manager::get_manager()
                .read()
                .unwrap()
                .restore_snapshot(snapshot);
            debug!("Restored pre-workflow mouse modifier snapshot");
        }

        // Disarm and restore previous armed command
        if implementation.armed_action.is_some() {
            self.disarm_and_restore();
        }

        // Clear stored state
        self.stored_state = StoredState::default();

        crate::trace_console_msg(format!("Workflow deactivated: {}\n", name));

        info!(id = %id, "Workflow deactivated successfully");
        true
    }

    /// Toggle a workflow on/off
    pub fn toggle(&mut self, id: &str) -> Result<bool, String> {
        let currently_active = self.is_active(id);
        debug!(
            id = %id,
            currently_active = currently_active,
            active_workflow = ?self.active_workflow,
            "Toggle workflow called"
        );

        if currently_active {
            debug!(id = %id, "Workflow is active, deactivating...");
            self.deactivate();
            Ok(false)
        } else {
            debug!(id = %id, "Workflow is not active, activating...");
            self.activate(id)?;
            Ok(true)
        }
    }

    /// Store current state before activating a workflow
    fn store_current_state(&mut self, implementation: &WorkflowImplementation) {
        self.stored_state = StoredState::default();

        // Store REAPER toggle states
        for setting in &implementation.reaper_settings {
            if let Some(current) = self.get_reaper_toggle_state(&setting.command) {
                self.stored_state
                    .reaper_settings
                    .insert(setting.command.clone(), current);
            }
        }

        // Snapshot all mouse modifier values from the active profile so they can
        // be fully restored when the workflow deactivates — even for contexts not
        // directly touched by the workflow's overlays.
        if !implementation.mouse_overlays.is_empty() {
            let snapshot = mouse_manager::get_manager().read().unwrap().take_snapshot();
            debug!(
                count = snapshot.len(),
                "Stored pre-workflow mouse modifier snapshot"
            );
            self.stored_state.mouse_snapshot = Some(snapshot);
        }
    }

    /// Apply REAPER settings for a workflow
    fn apply_reaper_settings(&self, settings: &[ReaperSetting]) {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        for setting in settings {
            // Get current state
            let current = self.get_reaper_toggle_state(&setting.command);

            // Only toggle if state doesn't match desired
            if current != Some(setting.enabled) {
                // Try to run the command
                if let Some(cmd_id) = self.resolve_command_id(&setting.command) {
                    debug!(
                        setting = %setting.name,
                        command = %setting.command,
                        desired = setting.enabled,
                        "Toggling REAPER setting"
                    );
                    medium.low().Main_OnCommand(cmd_id, 0);
                } else {
                    warn!(command = %setting.command, "Could not resolve command ID");
                }
            }
        }
    }

    /// Restore REAPER settings to their previous state
    fn restore_reaper_settings(&self, settings: &[ReaperSetting]) {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        for setting in settings {
            if let Some(&original) = self
                .stored_state
                .reaper_settings
                .get(setting.command.as_str())
            {
                let current = self.get_reaper_toggle_state(&setting.command);

                // Only toggle if state doesn't match original
                if current != Some(original)
                    && let Some(cmd_id) = self.resolve_command_id(&setting.command)
                {
                    debug!(
                        setting = %setting.name,
                        command = %setting.command,
                        restoring_to = original,
                        "Restoring REAPER setting"
                    );
                    medium.low().Main_OnCommand(cmd_id, 0);
                }
            }
        }
    }

    /// Get the current toggle state of a REAPER command
    fn get_reaper_toggle_state(&self, command: &str) -> Option<bool> {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        let cmd_id = self.resolve_command_id(command)?;

        let state = medium.low().GetToggleCommandState(cmd_id);
        // -1 = not a toggle, 0 = off, 1 = on
        if state >= 0 { Some(state == 1) } else { None }
    }

    /// Resolve a command string to a command ID
    fn resolve_command_id(&self, command: &str) -> Option<i32> {
        // Try parsing as numeric first
        if let Ok(id) = command.parse::<i32>() {
            return Some(id);
        }

        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        // Try using the action registry's stored command IDs first
        if let Some(cmd_id) = crate::infrastructure::action_registry::get_command_id(command) {
            return Some(cmd_id.get() as i32);
        }

        // Try as named command with NamedCommandLookup
        if let Some(cmd_id) = medium.named_command_lookup(command) {
            return Some(cmd_id.get() as i32);
        }

        // Try with underscore prefix (REAPER sometimes uses this format)
        let underscore_name = format!("_{}", command);
        if let Some(cmd_id) = medium.named_command_lookup(underscore_name.as_str()) {
            return Some(cmd_id.get() as i32);
        }

        None
    }

    /// Arm an action (it will run on next click/mouse action)
    ///
    /// Note: Not all actions support REAPER's arm mechanism. Only actions designed
    /// for mouse-position operations (like "Insert media item at mouse position")
    /// are typically armable. For actions that don't support native arming, use
    /// `.with_click_intercept()` on the ArmedAction to intercept clicks instead.
    fn arm_action(&mut self, action: &ArmedAction) {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        // First, store the currently armed command (if any)
        self.stored_state.previous_armed = self.get_armed_command();

        // Resolve the command ID
        if let Some(cmd_id) = self.resolve_command_id(&action.command) {
            use std::ffi::CString;
            let section = CString::new(action.section.as_str()).unwrap_or_default();

            debug!(
                action = %action.name,
                command = %action.command,
                cmd_id = cmd_id,
                section = %action.section,
                "About to arm action"
            );

            // Try direct ArmCommand API
            unsafe {
                medium.low().ArmCommand(cmd_id, section.as_ptr());
            }

            // Verify arming worked
            if let Some((armed_id, armed_section)) = self.get_armed_command() {
                info!(
                    action = %action.name,
                    command = %action.command,
                    cmd_id = cmd_id,
                    armed_id = armed_id,
                    armed_section = %armed_section,
                    "Armed action for workflow (verified)"
                );
            } else {
                // Arming didn't work - this is expected for most custom actions
                if action.intercept_clicks {
                    info!(
                        action = %action.name,
                        cmd_id = cmd_id,
                        "Action not armable via REAPER API, using click interception instead"
                    );
                } else {
                    warn!(
                        action = %action.name,
                        cmd_id = cmd_id,
                        "Action not armable - use keybinds or add .with_click_intercept()"
                    );
                }
            }
        } else {
            warn!(command = %action.command, "Could not resolve command ID for arming");
        }
    }

    /// Disarm and restore previously armed command
    fn disarm_and_restore(&mut self) {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        // Disarm current
        unsafe {
            let empty = std::ffi::CString::new("").unwrap();
            medium.low().ArmCommand(0, empty.as_ptr());
        }
        debug!("Disarmed action");

        // Restore previous if there was one
        if let Some((prev_cmd, prev_section)) = self.stored_state.previous_armed.take()
            && prev_cmd != 0
        {
            use std::ffi::CString;
            let section = CString::new(prev_section).unwrap_or_default();

            unsafe {
                medium.low().ArmCommand(prev_cmd, section.as_ptr());
            }
            debug!(cmd_id = prev_cmd, "Restored previously armed command");
        }
    }

    /// Get the currently armed command (cmd_id, section)
    fn get_armed_command(&self) -> Option<(i32, String)> {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        let mut section_buf = vec![0i8; 256];

        unsafe {
            let cmd_id = medium
                .low()
                .GetArmedCommand(section_buf.as_mut_ptr(), section_buf.len() as i32);

            if cmd_id == 0 {
                return None;
            }

            // Convert section buffer to string
            let null_pos = section_buf
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(section_buf.len());
            let section_bytes: Vec<u8> = section_buf[..null_pos].iter().map(|&b| b as u8).collect();
            let section = String::from_utf8(section_bytes).unwrap_or_default();

            Some((cmd_id, section))
        }
    }
}

impl Default for WorkflowManager {
    fn default() -> Self {
        Self::new()
    }
}

// === Global Manager Instance ===

static MANAGER: OnceLock<RwLock<WorkflowManager>> = OnceLock::new();

/// Get the global workflow manager (lazily initialized with defaults)
pub fn get_manager() -> &'static RwLock<WorkflowManager> {
    MANAGER.get_or_init(|| {
        let mut manager = WorkflowManager::new();

        // Register built-in workflows
        register_default_workflows(&mut manager);

        info!(
            definitions = manager.definitions.len(),
            default_impls = manager.default_implementations.len(),
            profile_impls = manager.profile_implementations.len(),
            "Workflow manager initialized"
        );

        RwLock::new(manager)
    })
}

/// Initialize the workflow manager
pub fn init() {
    let _ = get_manager();
}

/// Activate a workflow by ID
pub fn activate(id: &str) -> Result<(), String> {
    let mut manager = get_manager().write().unwrap();
    manager.activate(id)
}

/// Deactivate the current workflow
pub fn deactivate() -> bool {
    let mut manager = get_manager().write().unwrap();
    manager.deactivate()
}

/// Toggle a workflow on/off
pub fn toggle(id: &str) -> Result<bool, String> {
    let mut manager = get_manager().write().unwrap();
    manager.toggle(id)
}

/// Check if a workflow is active
pub fn is_active(id: &str) -> bool {
    let manager = get_manager().read().unwrap();
    manager.is_active(id)
}

/// Get the active workflow name (if any)
pub fn active_workflow_name() -> Option<String> {
    let manager = get_manager().read().unwrap();
    manager.active_definition().map(|d| d.name.to_string())
}

/// Get the active workflow name (alias for active_workflow_name)
pub fn get_active_workflow_name() -> Option<String> {
    active_workflow_name()
}

/// Get the active workflow display name (short name if applicable)
///
/// Uses the short name if the full name is > 12 chars with no spaces.
pub fn get_active_workflow_display_name() -> Option<String> {
    let manager = get_manager().read().unwrap();
    manager
        .active_definition()
        .map(|d| d.display_name().to_string())
}

/// Get the active workflow ID (if any)
pub fn get_active_workflow_id() -> Option<String> {
    let manager = get_manager().read().unwrap();
    manager.active_workflow.clone()
}

/// List all registered workflows
/// Returns a list of (id, name, description) tuples
pub fn list_workflows() -> Vec<(String, String, String)> {
    let manager = get_manager().read().unwrap();
    manager
        .list()
        .iter()
        .map(|(id, name, desc)| (id.to_string(), name.to_string(), desc.to_string()))
        .collect()
}

/// Get the click action for the active workflow (if any)
/// Only returns an action if click interception is enabled for the armed action
/// This is used by the mouse hook to trigger actions on left-click
/// DEPRECATED: Use get_armed_click_action() instead
pub fn get_click_action() -> Option<String> {
    let manager = get_manager().read().unwrap();
    manager
        .active_implementation
        .as_ref()
        .and_then(|impl_| impl_.armed_action.as_ref())
        .filter(|a| a.intercept_clicks)
        .map(|a| a.command.to_string())
}

/// Get the armed click action for the active workflow (if any)
/// This is the preferred method for click interception - it returns the
/// ArmedClickAction which includes context matching logic
pub fn get_armed_click_action() -> Option<ArmedClickAction> {
    let manager = get_manager().read().unwrap();
    manager
        .active_implementation
        .as_ref()
        .and_then(|impl_| impl_.armed_click.clone())
}

// === Built-in Workflows ===

/// Load workflow definitions from a directory of `.styx` files.
///
/// Each `*.styx` file in `dir` is parsed as a [`WorkflowConfig`].  Any inline
/// `bindings`/`wheel` in the file are registered as a keybind overlay named
/// `workflow-<id>` so the implementation can reference them.
///
/// Loaded workflows are registered as default implementations (profile-agnostic).
/// Profile-specific overrides can still be added in Rust via
/// `manager.implement_for_profile(...)`.
pub fn load_workflows_from_dir(dir: &std::path::Path) {
    let mut manager = get_manager().write().unwrap();
    load_workflows_from_dir_into(&mut manager, dir);
}

fn load_workflows_from_dir_into(manager: &mut WorkflowManager, dir: &Path) {
    use crate::input::processor;
    use crate::keybind_config::load_workflow_config;

    if !dir.exists() {
        info!(path = %dir.display(), "Workflows directory does not exist — skipping");
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(d) => d,
        Err(e) => {
            warn!(path = %dir.display(), "Cannot read workflows dir: {e}");
            return;
        }
    };

    // Collect (path, id, cfg) so we can register inline overlays before locking
    // the manager — the processor lock must not be held while we acquire manager.
    let mut pending: Vec<(String, crate::keybind_config::WorkflowConfig)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("styx") {
            continue;
        }

        // Derive the workflow ID from the filename stem: `tempo-mapping.styx` → `tempo-mapping`
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let Some(cfg) = load_workflow_config(&path) else {
            continue;
        };

        // If there are inline bindings, register an auto-named keybind overlay
        // BEFORE acquiring the manager lock.
        if cfg.has_inline_bindings() {
            let overlay_name = crate::keybind_config::WorkflowConfig::inline_overlay_name(&id);
            let overlay = crate::input::keybinds::KeybindOverride::new(
                &overlay_name,
                format!("Inline overlay for workflow '{id}'"),
            )
            .with_priority(10)
            .with_bindings(cfg.bindings().iter().map(|b| {
                let mut kb = crate::input::keybinds::Keybind::new(&b.keys, &b.action);
                if let Some(ref d) = b.desc {
                    kb = kb.with_description(d.clone());
                }
                if let Some(ctx) = b.context.clone() {
                    kb = kb.with_context(ctx);
                }
                kb
            }))
            .with_wheel_bindings(cfg.wheel().iter().map(|w| {
                let mut wb = crate::input::keybinds::WheelBind::new(&w.modifiers, &w.action);
                if w.horizontal.unwrap_or(false) {
                    wb = wb.with_horizontal();
                }
                if let Some(ref d) = w.desc {
                    wb = wb.with_description(d.clone());
                }
                if let Some(ctx) = w.context.clone() {
                    wb = wb.with_context(ctx);
                }
                wb
            }));

            processor::get_processor()
                .write()
                .unwrap()
                .add_override(overlay);
            info!("Registered inline keybind overlay '{overlay_name}' for workflow '{id}'");
        }

        // If there are inline mouse_settings, register a mouse modifier override
        // BEFORE acquiring the manager lock.
        if cfg.has_inline_mouse_settings() {
            use crate::input::mouse_modifiers::core::parse_modifier_flags;
            let overlay_name = crate::keybind_config::WorkflowConfig::inline_overlay_name(&id);
            let name_static: &'static str = Box::leak(overlay_name.clone().into_boxed_str());
            let desc_static: &'static str =
                Box::leak(format!("Inline mouse settings for workflow '{id}'").into_boxed_str());
            let mut mouse_override =
                crate::input::mouse_modifiers::manager::MouseModifierOverride::new(
                    name_static,
                    desc_static,
                )
                .with_priority(10);
            for def in cfg.mouse_settings() {
                let ctx: &'static str = Box::leak(def.ctx.clone().into_boxed_str());
                let behavior: &'static str = Box::leak(def.behavior.clone().into_boxed_str());
                let flags = parse_modifier_flags(&def.mods);
                let mut setting = crate::input::mouse_modifiers::manager::MouseModifierSetting::new(
                    ctx, flags, behavior,
                );
                if let Some(ref d) = def.desc {
                    let d_static: &'static str = Box::leak(d.clone().into_boxed_str());
                    setting = setting.with_description(d_static);
                }
                mouse_override = mouse_override.with_setting(setting);
            }
            mouse_manager::get_manager()
                .write()
                .unwrap()
                .register_override(mouse_override);
            info!("Registered inline mouse override '{overlay_name}' for workflow '{id}'");
        }

        pending.push((id, cfg));
    }

    // Now register all definitions and implementations under the manager lock.
    let mut loaded = 0usize;

    for (id, cfg) in pending {
        // Derive display name: `tempo-mapping` → `Tempo Mapping`
        let display_name = cfg
            .name
            .clone()
            .unwrap_or_else(|| crate::keybind_config::types::kebab_to_title(&id));
        let description = cfg.description.clone().unwrap_or_default();

        // Build keybind overlay list — explicit refs + optional inline overlay.
        let mut keybind_overlays: Vec<String> = cfg.keybind_overlays().to_vec();
        if cfg.has_inline_bindings() {
            keybind_overlays.push(crate::keybind_config::WorkflowConfig::inline_overlay_name(
                &id,
            ));
        }

        // Build mouse overlay list — explicit refs + optional inline mouse override.
        let mut mouse_overlays: Vec<String> = cfg.mouse_overlays().to_vec();
        if cfg.has_inline_mouse_settings() {
            mouse_overlays.push(crate::keybind_config::WorkflowConfig::inline_overlay_name(
                &id,
            ));
        }

        let mut impl_ = WorkflowImplementation::new()
            .with_keybind_overlays(keybind_overlays)
            .with_mouse_overlays(mouse_overlays);

        for setting in cfg.settings() {
            let desc = setting.desc.as_deref().unwrap_or(setting.command.as_str());
            impl_ = impl_.with_setting(ReaperSetting::new(
                desc,
                setting.command.clone(),
                setting.enabled,
            ));
        }

        if let Some(armed) = cfg.armed_action() {
            if armed.intercept_clicks == Some(true) {
                // FTS intercepts the item click itself, runs the action, then
                // passes the click through so REAPER's native drag (e.g. slip)
                // proceeds on the now-split/selected item — one gesture.
                impl_ = impl_.with_armed_click(armed_click_from_def(armed));
            } else {
                impl_ = impl_.with_armed_action(armed_action_from_def(armed));
            }
        }

        // WorkflowDefinition uses &'static str — Box::leak is intentional here since
        // these strings live for the duration of the process.
        manager.definitions.insert(
            id.clone(),
            WorkflowDefinition::new(
                Box::leak(id.clone().into_boxed_str()),
                Box::leak(display_name.into_boxed_str()),
                Box::leak(description.into_boxed_str()),
            ),
        );
        manager.default_implementations.insert(id.clone(), impl_);

        info!("Loaded workflow '{id}'");
        loaded += 1;
    }

    info!("Loaded {loaded} workflow(s) from {dir:?}");
}

/// Load profile-specific workflow implementations from a keybinds config directory.
///
/// Scans each profile subdirectory for a `workflows/` subdirectory. Each `.styx`
/// file inside is parsed as a [`WorkflowConfig`] and registered as a
/// profile-specific implementation (workflow ID from filename, profile from the
/// parent directory name). The workflow must already be defined globally — files
/// referencing undefined workflows are skipped with a warning.
///
/// Call this after [`load_workflows_from_dir`] so global definitions exist.
pub fn load_profile_workflows_from_dir(keybinds_dir: &std::path::Path) {
    let mut manager = get_manager().write().unwrap();
    load_profile_workflows_from_dir_into(&mut manager, keybinds_dir);
}

fn load_profile_workflows_from_dir_into(manager: &mut WorkflowManager, keybinds_dir: &Path) {
    if !keybinds_dir.exists() {
        return;
    }

    let read_dir = match std::fs::read_dir(keybinds_dir) {
        Ok(d) => d,
        Err(e) => {
            warn!("Cannot read keybinds dir for profile workflows: {e}");
            return;
        }
    };

    for entry in read_dir.flatten() {
        let profile_dir = entry.path();
        if !profile_dir.is_dir() {
            continue;
        }
        if entry.file_name() == "overlays" {
            continue;
        }

        let profile_name = entry.file_name().to_string_lossy().to_string();
        let workflows_subdir = profile_dir.join("workflows");
        if !workflows_subdir.is_dir() {
            continue;
        }

        load_one_profile_workflows(manager, &workflows_subdir, &profile_name);
    }
}

fn load_one_profile_workflows(
    manager: &mut WorkflowManager,
    dir: &std::path::Path,
    profile_name: &str,
) {
    use crate::input::processor;
    use crate::keybind_config::load_workflow_config;

    let entries = match std::fs::read_dir(dir) {
        Ok(d) => d,
        Err(e) => {
            warn!("Cannot read profile workflows dir {dir:?}: {e}");
            return;
        }
    };

    let mut pending: Vec<(String, crate::keybind_config::WorkflowConfig)> = Vec::new();

    // Phase 1: parse files and register inline overlays before acquiring the manager lock.
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("styx") {
            continue;
        }

        let id = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        let Some(cfg) = load_workflow_config(&path) else {
            continue;
        };

        if cfg.has_inline_bindings() {
            let overlay_name = format!("workflow-{profile_name}-{id}");
            let overlay = crate::input::keybinds::KeybindOverride::new(
                &overlay_name,
                format!("Inline overlay for {profile_name}/{id}"),
            )
            .with_priority(10)
            .with_bindings(cfg.bindings().iter().map(|b| {
                let mut kb = crate::input::keybinds::Keybind::new(&b.keys, &b.action);
                if let Some(ref d) = b.desc {
                    kb = kb.with_description(d.clone());
                }
                if let Some(ctx) = b.context.clone() {
                    kb = kb.with_context(ctx);
                }
                kb
            }))
            .with_wheel_bindings(cfg.wheel().iter().map(|w| {
                let mut wb = crate::input::keybinds::WheelBind::new(&w.modifiers, &w.action);
                if w.horizontal.unwrap_or(false) {
                    wb = wb.with_horizontal();
                }
                if let Some(ref d) = w.desc {
                    wb = wb.with_description(d.clone());
                }
                if let Some(ctx) = w.context.clone() {
                    wb = wb.with_context(ctx);
                }
                wb
            }));
            processor::get_processor()
                .write()
                .unwrap()
                .add_override(overlay);
        }

        if cfg.has_inline_mouse_settings() {
            use crate::input::mouse_modifiers::core::parse_modifier_flags;
            let overlay_name = format!("workflow-{profile_name}-{id}");
            let name_static: &'static str = Box::leak(overlay_name.clone().into_boxed_str());
            let desc_static: &'static str = Box::leak(
                format!("Inline mouse settings for {profile_name}/{id}").into_boxed_str(),
            );
            let mut mouse_override =
                crate::input::mouse_modifiers::manager::MouseModifierOverride::new(
                    name_static,
                    desc_static,
                )
                .with_priority(10);
            for def in cfg.mouse_settings() {
                let ctx: &'static str = Box::leak(def.ctx.clone().into_boxed_str());
                let behavior: &'static str = Box::leak(def.behavior.clone().into_boxed_str());
                let flags = parse_modifier_flags(&def.mods);
                let mut setting = crate::input::mouse_modifiers::manager::MouseModifierSetting::new(
                    ctx, flags, behavior,
                );
                if let Some(ref d) = def.desc {
                    let d_static: &'static str = Box::leak(d.clone().into_boxed_str());
                    setting = setting.with_description(d_static);
                }
                mouse_override = mouse_override.with_setting(setting);
            }
            mouse_manager::get_manager()
                .write()
                .unwrap()
                .register_override(mouse_override);
        }

        pending.push((id, cfg));
    }

    let mut loaded = 0usize;

    for (id, cfg) in pending {
        if !manager.definitions.contains_key(&id) {
            warn!(
                "Profile workflow '{id}' for '{profile_name}' references undefined workflow — skipping"
            );
            continue;
        }

        let mut keybind_overlays: Vec<String> = cfg.keybind_overlays().to_vec();
        if cfg.has_inline_bindings() {
            keybind_overlays.push(format!("workflow-{profile_name}-{id}"));
        }

        let mut mouse_overlays: Vec<String> = cfg.mouse_overlays().to_vec();
        if cfg.has_inline_mouse_settings() {
            mouse_overlays.push(format!("workflow-{profile_name}-{id}"));
        }

        let mut impl_ = WorkflowImplementation::new()
            .with_keybind_overlays(keybind_overlays)
            .with_mouse_overlays(mouse_overlays);

        for setting in cfg.settings() {
            let desc = setting.desc.as_deref().unwrap_or(setting.command.as_str());
            impl_ = impl_.with_setting(ReaperSetting::new(
                desc,
                setting.command.clone(),
                setting.enabled,
            ));
        }

        if let Some(armed) = cfg.armed_action() {
            if armed.intercept_clicks == Some(true) {
                // FTS intercepts the item click itself, runs the action, then
                // passes the click through so REAPER's native drag (e.g. slip)
                // proceeds on the now-split/selected item — one gesture.
                impl_ = impl_.with_armed_click(armed_click_from_def(armed));
            } else {
                impl_ = impl_.with_armed_action(armed_action_from_def(armed));
            }
        }

        manager.implement_for_profile(&id, profile_name, impl_);
        info!("Loaded profile workflow '{id}' for '{profile_name}'");
        loaded += 1;
    }

    if loaded > 0 {
        info!("Loaded {loaded} profile workflow(s) for '{profile_name}'");
    }
}

/// Build an [`ArmedClickAction`] from a styx [`ArmedActionDef`] with
/// `intercept_clicks true`: FTS catches the item click, runs the action, and
/// passes the click through so the native item drag (slip) continues.
fn armed_click_from_def(def: &crate::keybind_config::types::ArmedActionDef) -> ArmedClickAction {
    if def.slip_drag == Some(true) {
        // FTS drives the slip itself: execute the action (split), then run our
        // own slip-edit drag on the right piece — no pass-through, so REAPER's
        // edge-detection never sees the click.
        ArmedClickAction::on_item(def.command.clone()).with_slip_drag(true)
    } else {
        ArmedClickAction::on_item(def.command.clone()).with_pass_through(true)
    }
}

/// Build an [`ArmedAction`] from its styx [`ArmedActionDef`].
fn armed_action_from_def(def: &crate::keybind_config::types::ArmedActionDef) -> ArmedAction {
    let name = def.name.clone().unwrap_or_else(|| def.command.clone());
    let mut action = ArmedAction::new(name, def.command.clone());
    if let Some(section) = def.section.as_deref().filter(|s| !s.is_empty()) {
        action = action.with_section(section);
    }
    if def.intercept_clicks == Some(true) {
        action = action.with_click_intercept();
    }
    action
}

/// Rebuild workflow definitions and implementations from disk.
///
/// This clears file-backed workflow definitions so deletions and renames are
/// reflected immediately. Inline keybind/mouse overlays are re-registered into
/// the live processor and mouse modifier manager as part of the reload.
pub fn reload_from_dirs(workflows_dir: &Path, keybinds_dir: &Path) {
    // Snapshot the workflow IDs that existed *before* this reload so we
    // can fire the action registrar for any new ones. Done before the
    // lock to keep the critical section minimal.
    let before: std::collections::HashSet<String> = list_workflows()
        .into_iter()
        .map(|(id, _, _)| id.to_string())
        .collect();

    {
        let mut manager = get_manager().write().unwrap();
        let mut fresh = WorkflowManager::new();
        register_default_workflows(&mut fresh);
        load_workflows_from_dir_into(&mut fresh, workflows_dir);
        load_profile_workflows_from_dir_into(&mut fresh, keybinds_dir);
        *manager = fresh;
    }

    // Diff and fire late-registration for any newly-loaded workflows.
    // Drops + re-adds (same id) don't fire — they don't need a new
    // REAPER named command, just a re-application of the overlay.
    let after = list_workflows();
    for (id, name, _desc) in &after {
        let id_str = id.to_string();
        if before.contains(&id_str) {
            continue;
        }
        let def = build_dynamic_workflow_def(&id_str, name);
        tracing::info!(
            workflow = id,
            "[workflows] Late-registering action for newly-loaded workflow"
        );
        crate::infrastructure::action_registry::fire_action_registrar(def);
    }
}

/// Build the [`DynActionDef`] for a workflow's toggle action. Mirrors
/// the closures in [`get_dynamic_workflow_action_defs`] in
/// `input::actions` so a late-registered workflow has the same shape
/// as one registered at startup.
fn build_dynamic_workflow_def(
    id: &str,
    name: &str,
) -> crate::infrastructure::action_registry::DynActionDef {
    use crate::infrastructure::action_registry::{ActionSection, DynActionDef};
    use std::sync::Arc;

    let command_id = format!("FTS_INPUT_WORKFLOW_{}", id.to_uppercase().replace('-', "_"));
    let id_toggle = id.to_string();
    let id_state = id.to_string();
    DynActionDef {
        command_id,
        display_name: format!("Workflow: {}", name),
        handler: Arc::new(move || {
            if let Err(e) = toggle(&id_toggle) {
                tracing::warn!(
                    workflow = %id_toggle,
                    error = %e,
                    "Failed to toggle workflow",
                );
            }
        }),
        appears_in_menu: true,
        section: ActionSection::Main,
        toggle_state: Some(Arc::new(move || is_active(&id_state))),
    }
}

fn register_default_workflows(manager: &mut WorkflowManager) {
    // ============================================
    // Tempo Mapping Workflow
    // ============================================

    // Define the workflow (profile-agnostic)
    manager.define(
        "tempo-mapping",
        "Tempo Mapping",
        "Workflow for tempo mapping audio to the grid",
    );

    // Default implementation (used by any profile without a specific override)
    manager.implement(
        "tempo-mapping",
        WorkflowImplementation::new()
            .with_keybind_overlay("tempo-map")
            .with_mouse_overlay("tempo-map"),
    );

    // ============================================
    // Fast Slip Edit Workflow
    // ============================================

    // Define the workflow
    manager.define(
        "fast-slip-edit",
        "Fast Slip Edit",
        "Quick slip editing workflow for tight timing adjustments",
    );

    // Default implementation
    manager.implement(
        "fast-slip-edit",
        WorkflowImplementation::new()
            .with_keybind_overlay("fast-slip-edit")
            .with_mouse_overlay("fast-slip-edit")
            // Disable snapping for precise slip editing
            .with_setting(ReaperSetting::off("Snapping", "1157"))
            // Enable auto-crossfade on split for seamless edits
            .with_setting(ReaperSetting::on("Auto-crossfade on split", "40912"))
            // Custom click interception - clicking on items triggers split with crossfade
            .with_armed_click(ArmedClickAction::on_item("FTS_SPLIT_ITEMS_CROSSFADE_LEFT")),
    );

    // Profile-specific implementations are loaded from
    // config/<profile>/workflows/*.styx via load_profile_workflows_from_dir.
}
