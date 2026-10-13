//! FTS-Input Actions
//!
//! Actions for controlling the FTS-Input key sequence system.

use crate::infrastructure::action_registry::{ActionDef, DynActionDef, register_actions};
use crate::infrastructure::workflow_selector;
use crate::input::handler::InputHandler;
use crate::input::keybinds;
use crate::input::mouse_modifiers::core::{MouseModifierFlag, set_mouse_modifier};
use crate::input::mouse_modifiers::manager as mouse_manager;
use crate::input::mouse_modifiers::preset as mouse_preset;
use crate::input::workflows;
use reaper_high::Reaper;
use tracing::{debug, info};

/// Toggle FTS Input Status Panel
fn toggle_status_panel_handler() {
    reaper_dioxus::toggle_panel("FTS_INPUT_STATUS");
}

/// Get the current toggle state for FTS Input Status Panel
fn get_status_panel_state() -> bool {
    reaper_dioxus::is_panel_visible("FTS_INPUT_STATUS")
}

/// Toggle FTS-Input interception on/off
fn toggle_input_interception_handler() {
    let is_enabled = InputHandler::toggle();
    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    if is_enabled {
        match mouse_preset::backup_current_modifiers_if_missing(medium_reaper) {
            Ok(true) => info!("Created user mouse modifier backup"),
            Ok(false) => debug!("User mouse modifier backup already exists"),
            Err(e) => tracing::warn!("Failed to create user mouse modifier backup: {}", e),
        }
    } else {
        mouse_manager::disable_all_overrides();
        if let Err(e) = mouse_preset::restore_user_backup(medium_reaper) {
            tracing::warn!("Failed to restore user mouse modifiers from backup: {}", e);
            crate::trace_console_msg(format!(
                "FTS-Input: failed to restore mouse modifier backup ({})\n",
                e
            ));
        } else {
            crate::trace_console_msg("FTS-Input: restored mouse modifiers from backup\n");
            info!("Restored user mouse modifiers from backup");
        }
    }

    let status = if is_enabled { "enabled" } else { "disabled" };
    crate::trace_console_msg(format!("FTS-Input interception: {}\n", status));
    info!("FTS-Input interception toggled: {}", status);

    // Wake up REAPER to refresh action states
    if let Err(e) = reaper.wake_up() {
        tracing::warn!("Failed to wake up REAPER after toggle: {}", e);
    }
}

/// Get the current toggle state for FTS-Input interception
fn get_input_interception_state() -> bool {
    InputHandler::is_enabled()
}

/// Explicit "disable FTS Input" handler. Idempotent — does the same
/// teardown the toggle does when going on→off, but always lands in
/// the disabled state regardless of current state. Convenient to bind
/// to a single keystroke (or just run via the Action list) when you
/// want REAPER's native keymap back without thinking about state.
fn disable_input_handler() {
    if !InputHandler::is_enabled() {
        crate::trace_console_msg("FTS-Input already disabled.\n");
        return;
    }
    // Same teardown path as the toggle's on→off branch.
    let medium_reaper = Reaper::get().medium_reaper();
    mouse_manager::disable_all_overrides();
    if let Err(e) = mouse_preset::restore_user_backup(medium_reaper) {
        tracing::warn!("Failed to restore user mouse modifiers from backup: {}", e);
        crate::trace_console_msg(format!(
            "FTS-Input: failed to restore mouse modifier backup ({})\n",
            e
        ));
    } else {
        crate::trace_console_msg("FTS-Input: restored mouse modifiers from backup\n");
        info!("Restored user mouse modifiers from backup");
    }
    InputHandler::set_enabled(false);
    crate::trace_console_msg("FTS-Input interception: disabled\n");
    info!("FTS-Input interception explicitly disabled");
    if let Err(e) = Reaper::get().wake_up() {
        tracing::warn!("Failed to wake up REAPER after disable: {}", e);
    }
}

/// Explicit "enable FTS Input" counterpart.
fn enable_input_handler() {
    if InputHandler::is_enabled() {
        crate::trace_console_msg("FTS-Input already enabled.\n");
        return;
    }
    let medium_reaper = Reaper::get().medium_reaper();
    match mouse_preset::backup_current_modifiers_if_missing(medium_reaper) {
        Ok(true) => info!("Created user mouse modifier backup"),
        Ok(false) => debug!("User mouse modifier backup already exists"),
        Err(e) => tracing::warn!("Failed to create user mouse modifier backup: {}", e),
    }
    InputHandler::set_enabled(true);
    crate::trace_console_msg("FTS-Input interception: enabled\n");
    info!("FTS-Input interception explicitly enabled");
    if let Err(e) = Reaper::get().wake_up() {
        tracing::warn!("Failed to wake up REAPER after enable: {}", e);
    }
}

/// Toggle FTS-Input passthrough mode on/off
fn toggle_input_passthrough_handler() {
    let is_passthrough = InputHandler::toggle_passthrough();
    let reaper = Reaper::get();

    let status = if is_passthrough {
        "enabled (logging only)"
    } else {
        "disabled (intercepting)"
    };
    crate::trace_console_msg(format!("FTS-Input passthrough mode: {}\n", status));
    info!("FTS-Input passthrough mode toggled: {}", status);

    // Wake up REAPER to refresh action states
    if let Err(e) = reaper.wake_up() {
        tracing::warn!("Failed to wake up REAPER after toggle: {}", e);
    }
}

/// Get the current toggle state for FTS-Input passthrough mode
fn get_input_passthrough_state() -> bool {
    InputHandler::is_passthrough()
}

/// Toggle FTS-Input debug logging on/off
fn toggle_input_debug_logging_handler() {
    let is_enabled = InputHandler::toggle_debug_logging();
    info!(
        "FTS-Input debug logging toggled: {}",
        if is_enabled { "enabled" } else { "disabled" }
    );

    // Wake up REAPER to refresh action states
    if let Err(e) = Reaper::get().wake_up() {
        tracing::warn!("Failed to wake up REAPER after toggle: {}", e);
    }
}

/// Get the current toggle state for FTS-Input debug logging
fn get_input_debug_logging_state() -> bool {
    InputHandler::is_debug_logging()
}

/// Toggle debug mouse context logging on/off
fn toggle_debug_mouse_context_handler() {
    let is_enabled = workflows::toggle_debug_mouse_context();
    let _reaper = Reaper::get();

    let status = if is_enabled { "enabled" } else { "disabled" };
    crate::trace_console_msg(format!(
        "Debug Mouse Context: {} (click anywhere to see context)\n",
        status
    ));
    info!("Debug Mouse Context toggled: {}", status);

    wake_reaper();
}

/// Get the current toggle state for debug mouse context
fn get_debug_mouse_context_state() -> bool {
    workflows::is_debug_mouse_context_enabled()
}

// === Keybind Preset Actions ===

/// Apply mouse modifier profile for a preset and log the state
fn apply_preset_mouse_modifiers(preset_name: &str) {
    // Map keybind preset names to mouse modifier profile names
    let profile_name = match preset_name.to_lowercase().as_str() {
        "fasttrackstudio" => "fasttrackstudio",
        "logic" => "logic",
        _ => "reaper",
    };

    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();
    match mouse_preset::backup_current_modifiers_if_missing(medium_reaper) {
        Ok(true) => info!("Created user mouse modifier backup"),
        Ok(false) => debug!("User mouse modifier backup already exists"),
        Err(e) => tracing::warn!("Failed to create user mouse modifier backup: {}", e),
    }

    // Apply the mouse modifier profile
    mouse_manager::set_profile(profile_name);
    mouse_manager::log_state();
}

// === Workflow Actions ===

fn deactivate_workflow_handler() {
    if workflows::deactivate() {
        info!("Workflow deactivated");
    } else {
        info!("No active workflow to deactivate");
    }
    wake_reaper();
}

fn reset_all_overrides_handler() {
    let _reaper = Reaper::get();

    // Deactivate any active workflow first
    workflows::deactivate();

    // Disable all keybind overlays
    keybinds::disable_all_overrides();

    // Disable all mouse modifier overlays
    mouse_manager::disable_all_overrides();

    crate::trace_console_msg("All overlays cleared, returned to base profile\n");
    info!("Reset all overrides - returned to base profile");

    // Log current state
    mouse_manager::log_state();

    wake_reaper();
}

fn reset_mouse_to_profile_handler() {
    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    // Disable all mouse modifier overlays (returns to base profile)
    mouse_manager::disable_all_overrides();

    if let Err(e) = mouse_preset::restore_user_backup(medium_reaper) {
        tracing::warn!("Failed to restore user mouse modifiers from backup: {}", e);
        crate::trace_console_msg(format!(
            "FTS: failed to restore mouse modifier backup ({})\n",
            e
        ));
    } else {
        crate::trace_console_msg("Mouse modifiers restored from backup\n");
        info!("Restored mouse modifiers from backup");
    }

    // Log current state
    mouse_manager::log_state();

    wake_reaper();
}

/// Helper to refresh REAPER UI after state changes
/// Uses RefreshToolbar2 to update toggle button states
fn refresh_toolbar(command_id: &str) {
    let reaper = Reaper::get();
    let medium = reaper.medium_reaper();

    // Try to get the command ID for our action
    if let Some(cmd_id) = crate::infrastructure::action_registry::get_command_id(command_id) {
        // Section 0 = main section
        medium.low().RefreshToolbar2(0, cmd_id.get() as i32);
        debug!(command_id = %command_id, cmd_id = cmd_id.get(), "Refreshed toolbar");
    }
}

/// Helper to wake up REAPER after state changes
fn wake_reaper() {
    if let Err(e) = Reaper::get().wake_up() {
        tracing::warn!("Failed to wake up REAPER: {}", e);
    }
}

// === Dev Actions for Mouse Modifier Discovery ===

/// Dev action: Test and log mouse modifier behavior IDs for MM_CTX_ITEM_CLK
/// Sets behavior IDs 0-30 to different modifier flags so you can check REAPER preferences
fn dev_test_mouse_modifier_ids_handler() {
    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    crate::trace_console_msg("\n=== Mouse Modifier Behavior ID Test ===\n");
    crate::trace_console_msg("Context: MM_CTX_ITEM_CLK (Media Item - left click)\n");
    crate::trace_console_msg("Setting behavior IDs to modifier flags for discovery...\n\n");

    // Test context - can be changed to test other contexts
    let context = "MM_CTX_ITEM_CLK";

    // Set IDs 16-25 to flags 0-9 to discover the unknown behaviors
    crate::trace_console_msg("Setting IDs 16-25 to discover unknown behaviors:\n");
    crate::trace_console_msg("  Flag 0 (Default) = ID 16\n");
    crate::trace_console_msg("  Flag 1 (Shift) = ID 17\n");
    crate::trace_console_msg("  Flag 2 (Cmd) = ID 18\n");
    crate::trace_console_msg("  Flag 3 (Shift+Cmd) = ID 19\n");
    crate::trace_console_msg("  Flag 4 (Alt) = ID 20\n");
    crate::trace_console_msg("  Flag 5 (Shift+Alt) = ID 21\n");
    crate::trace_console_msg("  Flag 6 (Cmd+Alt) = ID 22\n");
    crate::trace_console_msg("  Flag 7 (Shift+Cmd+Alt) = ID 23\n");
    crate::trace_console_msg("  Flag 8 (Win/Ctrl-Mac) = ID 24\n");
    crate::trace_console_msg("  Flag 9 (Shift+Win) = ID 25\n\n");

    // Set IDs 16-25 to flags 0-9
    for (i, id) in (16..=25).enumerate() {
        let flag = MouseModifierFlag::from_flag(i as i32);
        let behavior_str = format!("{} m", id);
        if let Err(e) = set_mouse_modifier(context, flag, &behavior_str, medium_reaper) {
            crate::trace_console_msg(format!("  Error setting ID {}: {}\n", id, e));
        }
    }

    crate::trace_console_msg(
        "Done! Open REAPER Preferences > Mouse Modifiers > Media item > left click\n",
    );
    crate::trace_console_msg("to see what behavior names correspond to each modifier.\n\n");
    crate::trace_console_msg("The modifier flags map as follows:\n");
    crate::trace_console_msg("  Default action = ID 16\n");
    crate::trace_console_msg("  Shift = ID 17\n");
    crate::trace_console_msg("  Cmd = ID 18\n");
    crate::trace_console_msg("  Shift+Cmd = ID 19\n");
    crate::trace_console_msg("  Opt = ID 20 (should be 'Extend razor edit area' per docs)\n");
    crate::trace_console_msg("  Shift+Opt = ID 21\n");
    crate::trace_console_msg("  Cmd+Opt = ID 22\n");
    crate::trace_console_msg("  Shift+Cmd+Opt = ID 23\n");
    crate::trace_console_msg("  Ctrl (Mac) / Win (Windows) = ID 24\n");
    crate::trace_console_msg("  Shift+Ctrl/Win = ID 25\n");

    info!("Mouse modifier behavior ID test complete - check REAPER preferences");
}

/// Dev action: Reset MM_CTX_ITEM_CLK to defaults
fn dev_reset_item_click_modifiers_handler() {
    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    crate::trace_console_msg("\n=== Resetting MM_CTX_ITEM_CLK to defaults ===\n");

    let context = "MM_CTX_ITEM_CLK";

    // Reset all 16 modifier flags to default (-1)
    for flag_num in 0..16 {
        let flag = MouseModifierFlag::from_flag(flag_num);
        let _ = set_mouse_modifier(context, flag, "-1", medium_reaper);
    }

    crate::trace_console_msg("Done! All Media Item click modifiers reset to defaults.\n");
    info!("MM_CTX_ITEM_CLK reset to defaults");
}

/// Get all FTS-Input action definitions (for batch registration)
pub fn get_input_action_defs() -> Vec<ActionDef> {
    vec![
        ActionDef {
            command_id: "FTS_INPUT_TOGGLE",
            display_name:
                "FTS: Enable/Disable Input system (toggle — off = REAPER handles keys natively)"
                    .to_string(),
            handler: toggle_input_interception_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: Some(get_input_interception_state),
        },
        // Non-toggle "always disable" alias for users who want a single
        // action they can bind / search for to step out of FTS-Input
        // and use REAPER's native keymap.
        ActionDef {
            command_id: "FTS_INPUT_DISABLE",
            display_name: "FTS: Disable Input system (use REAPER native keymap)".to_string(),
            handler: disable_input_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: None,
        },
        ActionDef {
            command_id: "FTS_INPUT_ENABLE",
            display_name: "FTS: Enable Input system (resume FTS keymap)".to_string(),
            handler: enable_input_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: None,
        },
        ActionDef {
            command_id: "FTS_INPUT_TOGGLE_PASSTHROUGH",
            display_name: "Toggle FTS-Input Passthrough Mode".to_string(),
            handler: toggle_input_passthrough_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: Some(get_input_passthrough_state),
        },
        ActionDef {
            command_id: "FTS_INPUT_TOGGLE_DEBUG_LOGGING",
            display_name: "Toggle FTS-Input Debug Logging (logs all keys to console)".to_string(),
            handler: toggle_input_debug_logging_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: Some(get_input_debug_logging_state),
        },
        ActionDef {
            command_id: "FTS_INPUT_DEBUG_MOUSE_CONTEXT",
            display_name: "Toggle Debug Mouse Context (logs MM_CTX on every click)".to_string(),
            handler: toggle_debug_mouse_context_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: Some(get_debug_mouse_context_state),
        },
        ActionDef {
            command_id: "FTS_INPUT_RESET_ALL_OVERRIDES",
            display_name: "FTS: Reset all overlays (return to base profile)".to_string(),
            handler: reset_all_overrides_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: None,
        },
        ActionDef {
            command_id: "FTS_INPUT_MOUSE_RESET_TO_PROFILE",
            display_name: "FTS: Reset mouse modifiers to base profile".to_string(),
            handler: reset_mouse_to_profile_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: None,
        },
        // === Profile Selector ===
        ActionDef {
            command_id: "FTS_INPUT_PROFILE_SELECTOR",
            display_name: "FTS: Profile Selector (popup menu)".to_string(),
            handler: workflow_selector::profile_selector_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: Some(workflow_selector::get_profile_selector_state),
        },
        // === Workflow Selector ===
        ActionDef {
            command_id: "FTS_INPUT_WORKFLOW_SELECTOR",
            display_name: "FTS: Workflow Selector (popup menu)".to_string(),
            handler: workflow_selector::workflow_selector_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: Some(workflow_selector::get_workflow_selector_state),
        },
        // === Panels ===
        ActionDef {
            command_id: "FTS_INPUT_TOGGLE_STATUS_PANEL",
            display_name: "FTS: Toggle Input Status Panel".to_string(),
            handler: toggle_status_panel_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: Some(get_status_panel_state),
        },
        // === Dev Actions for Mouse Modifier Discovery ===
        ActionDef {
            command_id: "FTS_INPUT_DEV_TEST_MOUSE_MODIFIER_IDS",
            display_name: "Test Mouse Modifier Behavior IDs (sets IDs 16-25 to discover names)"
                .to_string(),
            handler: dev_test_mouse_modifier_ids_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: None,
        },
        ActionDef {
            command_id: "FTS_INPUT_DEV_RESET_ITEM_CLICK_MODIFIERS",
            display_name: "Reset Media Item Click Modifiers to defaults".to_string(),
            handler: dev_reset_item_click_modifiers_handler,
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: None,
        },
    ]
}

/// Build dynamic preset action defs from the currently registered keybind presets.
///
/// Call this **after** all preset configs have been loaded (i.e. after
/// [`crate::load_keybind_presets_from`]). Returns one "set preset" action per
/// registered preset.
///
/// Command ID format: `FTS_INPUT_KEYBIND_SET_PRESET_<NAME_UPPER>` where `<NAME_UPPER>`
/// is the preset name uppercased — e.g. `fasttrackstudio` → `FTS_INPUT_KEYBIND_SET_PRESET_FASTTRACKSTUDIO`.
pub fn get_dynamic_preset_action_defs() -> Vec<DynActionDef> {
    use std::sync::Arc;

    keybinds::available_presets()
        .into_iter()
        .map(|(id, display_name)| {
            let command_id = format!(
                "FTS_INPUT_KEYBIND_SET_PRESET_{}",
                id.to_uppercase().replace('-', "_")
            );
            let id_clone = id.clone();
            let label = display_name.clone();
            let cmd_for_toolbar = command_id.clone();

            DynActionDef {
                command_id,
                display_name: format!("Set keybind preset: {}", display_name),
                handler: Arc::new(move || {
                    keybinds::set_preset(&id_clone);
                    apply_preset_mouse_modifiers(&id_clone);
                    crate::trace_console_msg(format!("Keybind preset: {}\n", label));
                    info!("Keybind preset changed to: {}", label);
                    refresh_toolbar(&cmd_for_toolbar);
                    wake_reaper();
                }),
                appears_in_menu: true,
                section: crate::infrastructure::action_registry::ActionSection::Main,
                toggle_state: None,
            }
        })
        .collect()
}

/// Build dynamic workflow action defs from the currently registered workflow definitions.
///
/// Call this **after** all workflow configs have been loaded (i.e. after
/// [`crate::load_workflows_from`] and [`crate::load_profile_workflows_from`]).
/// Returns one toggle action per workflow plus a single "deactivate" action.
///
/// Command ID format: `FTS_INPUT_WORKFLOW_<ID_UPPER>` where `<ID_UPPER>` is the
/// workflow id uppercased with hyphens replaced by underscores — e.g.
/// `tempo-mapping` → `FTS_INPUT_WORKFLOW_TEMPO_MAPPING`.
pub fn get_dynamic_workflow_action_defs() -> Vec<DynActionDef> {
    use std::sync::Arc;

    let workflow_list = workflows::list_workflows();
    let mut defs: Vec<DynActionDef> = Vec::with_capacity(workflow_list.len() + 1);

    for (id, name, _desc) in workflow_list {
        let command_id: String =
            format!("FTS_INPUT_WORKFLOW_{}", id.to_uppercase().replace('-', "_"));
        let id_toggle = id.clone();
        let id_state = id.clone();
        let cmd_for_toolbar = command_id.clone();

        defs.push(DynActionDef {
            command_id,
            display_name: format!("Workflow: {}", name),
            handler: Arc::new(move || {
                match workflows::toggle(&id_toggle) {
                    Ok(active) => {
                        info!(
                            "Workflow '{}' {}",
                            id_toggle,
                            if active { "activated" } else { "deactivated" }
                        );
                    }
                    Err(e) => tracing::warn!("Failed to toggle workflow '{}': {}", id_toggle, e),
                }
                refresh_toolbar(&cmd_for_toolbar);
                wake_reaper();
            }),
            appears_in_menu: true,
            section: crate::infrastructure::action_registry::ActionSection::Main,
            toggle_state: Some(Arc::new(move || workflows::is_active(&id_state))),
        });
    }

    // Generic deactivate — always present regardless of which workflows are loaded.
    defs.push(DynActionDef {
        command_id: "FTS_INPUT_WORKFLOW_DEACTIVATE".to_string(),
        display_name: "Workflow: Deactivate current workflow".to_string(),
        handler: Arc::new(deactivate_workflow_handler),
        appears_in_menu: true,
        section: crate::infrastructure::action_registry::ActionSection::Main,
        toggle_state: None,
    });

    defs
}

/// Register all FTS-Input actions (legacy - prefer using get_input_action_defs for batch registration)
pub fn register_input_actions() {
    info!("🎯 register_input_actions() called - registering FTS-Input actions");
    let actions = get_input_action_defs();
    info!("📝 Calling register_actions with {} actions", actions.len());
    register_actions(&actions, "Input");
    info!("✅ register_input_actions() completed");
}
