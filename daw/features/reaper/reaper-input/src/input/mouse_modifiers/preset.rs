//! Mouse modifier preset management
//!
//! Functions to save, load, list, and delete mouse modifier presets.
//! Presets are saved as JSON files in a presets directory.

use crate::input::mouse_modifiers::types::MouseButtonInput;

use crate::input::mouse_modifiers::contexts::ALL_CONTEXTS;
use crate::input::mouse_modifiers::core::{
    MouseModifierFlag, get_mouse_modifier, set_mouse_modifier,
};
use facet::Facet;
use reaper_high::Reaper;
use reaper_medium::Reaper as MediumReaper;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// System-managed backup name for restoring user state after FTS modifications.
pub const USER_BACKUP_PRESET_NAME: &str = "__fts_user_mouse_modifiers_backup";

/// Mouse modifier preset data structure
#[derive(Debug, Clone, Facet)]
pub struct MouseModifierPreset {
    pub name: String,
    pub description: Option<String>,
    pub modifiers: HashMap<String, HashMap<String, Option<String>>>,
    // Structure: modifiers[context][flag_string] = action_string
    // flag_string format: "shift_ctrl_alt_win" or "none"
}

impl MouseModifierPreset {
    pub fn new(name: String) -> Self {
        Self {
            name,
            description: None,
            modifiers: HashMap::new(),
        }
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }
}

/// Get the presets directory path
fn get_presets_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let reaper = Reaper::get();
    let resource_path = reaper.resource_path();
    let presets_dir = resource_path.join("FTS-MouseModifierPresets");

    // Create directory if it doesn't exist
    if !presets_dir.exists() {
        fs::create_dir_all(&presets_dir)?;
    }

    // Convert from camino::Utf8PathBuf to std::path::PathBuf
    Ok(presets_dir.into())
}

/// Convert modifier flag to string key
fn flag_to_key(flag: MouseModifierFlag) -> String {
    if flag.shift || flag.control || flag.alt || flag.win {
        let mut parts = Vec::new();
        if flag.shift {
            parts.push("shift");
        }
        if flag.control {
            parts.push("ctrl");
        }
        if flag.alt {
            parts.push("alt");
        }
        if flag.win {
            parts.push("win");
        }
        parts.join("_")
    } else {
        "none".to_string()
    }
}

/// Parse modifier flag from string key
fn key_to_flag(key: &str) -> MouseModifierFlag {
    if key == "none" {
        return MouseModifierFlag::none();
    }

    let parts: Vec<&str> = key.split('_').collect();
    MouseModifierFlag {
        shift: parts.contains(&"shift"),
        control: parts.contains(&"ctrl"),
        alt: parts.contains(&"alt"),
        win: parts.contains(&"win"),
    }
}

/// Get the full file path for a preset name.
fn preset_file_path(preset_name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let presets_dir = get_presets_dir()?;
    Ok(presets_dir.join(format!("{}.json", preset_name)))
}

/// Save all current mouse modifiers to a preset
pub fn save_all_modifiers(
    preset_name: &str,
    description: Option<&str>,
    medium_reaper: &MediumReaper,
) -> Result<MouseModifierPreset, Box<dyn std::error::Error>> {
    let mut preset = MouseModifierPreset::new(preset_name.to_string());

    if let Some(desc) = description {
        preset = preset.with_description(desc.to_string());
    }

    // Iterate through all contexts
    for context in &*ALL_CONTEXTS {
        let mut context_modifiers = HashMap::new();

        // Check all 16 possible modifier flag combinations (0-15)
        // Using the iterator for clarity
        use crate::input::mouse_modifiers::core::AllModifierFlags;
        for (_flag_val, flag) in AllModifierFlags::new() {
            if let Some(action) = get_mouse_modifier(context, flag, medium_reaper) {
                let flag_key = flag_to_key(flag);
                context_modifiers.insert(flag_key, Some(action));
            }
            // Note: We don't store unassigned modifiers to keep presets clean
        }

        // Only add context if it has at least one modifier assigned
        if !context_modifiers.is_empty() {
            preset
                .modifiers
                .insert(context.to_string(), context_modifiers);
        }
    }

    // Save to file
    let preset_file = preset_file_path(preset_name)?;
    let json = facet_json::to_string_pretty(&preset)?;
    fs::write(&preset_file, json)?;

    Ok(preset)
}

/// Load a preset and apply it to REAPER
pub fn load_preset(
    preset_name: &str,
    medium_reaper: &MediumReaper,
) -> Result<(), Box<dyn std::error::Error>> {
    let preset_file = preset_file_path(preset_name)?;

    if !preset_file.exists() {
        return Err(format!("Preset '{}' not found", preset_name).into());
    }

    let json = fs::read_to_string(&preset_file)?;
    let preset: MouseModifierPreset = facet_json::from_str(&json)?;

    // Apply all modifiers from the preset
    for (context, modifiers) in preset.modifiers {
        for (flag_key, action_opt) in modifiers {
            let flag = key_to_flag(&flag_key);

            if let Some(action) = action_opt {
                set_mouse_modifier(&context, flag, &action, medium_reaper)?;
            } else {
                // Reset this modifier
                set_mouse_modifier(&context, flag, "-1", medium_reaper)?;
            }
        }
    }

    Ok(())
}

/// List all available presets
pub fn list_presets() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let presets_dir = get_presets_dir()?;

    let mut presets = Vec::new();

    for entry in fs::read_dir(&presets_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("json")
            && let Some(name) = path.file_stem().and_then(|s| s.to_str())
        {
            presets.push(name.to_string());
        }
    }

    presets.sort();
    Ok(presets)
}

/// Delete a preset
pub fn delete_preset(preset_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let preset_file = preset_file_path(preset_name)?;

    if !preset_file.exists() {
        return Err(format!("Preset '{}' not found", preset_name).into());
    }

    fs::remove_file(&preset_file)?;
    Ok(())
}

/// Return the on-disk path for the system-managed backup preset.
pub fn user_backup_file_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    preset_file_path(USER_BACKUP_PRESET_NAME)
}

/// Save a full backup snapshot of the user's current mouse modifiers.
pub fn backup_current_modifiers(
    medium_reaper: &MediumReaper,
) -> Result<MouseModifierPreset, Box<dyn std::error::Error>> {
    save_all_modifiers(
        USER_BACKUP_PRESET_NAME,
        Some("Auto backup before FTS mouse modifier changes"),
        medium_reaper,
    )
}

/// Save the user backup only if one does not already exist.
pub fn backup_current_modifiers_if_missing(
    medium_reaper: &MediumReaper,
) -> Result<bool, Box<dyn std::error::Error>> {
    let backup_file = user_backup_file_path()?;
    if backup_file.exists() {
        return Ok(false);
    }
    let _ = backup_current_modifiers(medium_reaper)?;
    Ok(true)
}

/// Restore the user's full mouse modifier state from backup.
pub fn restore_user_backup(medium_reaper: &MediumReaper) -> Result<(), Box<dyn std::error::Error>> {
    load_preset(USER_BACKUP_PRESET_NAME, medium_reaper)
}

/// Test a specific context to see if GetMouseModifier works
#[allow(dead_code)]
fn test_context(context: &str, medium_reaper: &MediumReaper) -> bool {
    use crate::input::mouse_modifiers::core::{MouseModifierFlag, get_mouse_modifier};

    // Try the default action (flag 0)
    let flag = MouseModifierFlag::none();
    if let Some(action) = get_mouse_modifier(context, flag, medium_reaper) {
        crate::trace_console_msg(format!(
            "TEST: Context '{}' returned action: '{}'\n",
            context, action
        ));
        return true;
    }
    false
}

/// Log all current mouse modifiers to console
pub fn log_all_modifiers(medium_reaper: &MediumReaper) {
    let low_reaper = medium_reaper.low();

    // Check if GetMouseModifier is available
    if low_reaper.pointers().GetMouseModifier.is_none() {
        crate::trace_console_msg(
            "ERROR: GetMouseModifier is not available in this REAPER version\n",
        );
        crate::trace_console_msg("Mouse modifier functions require REAPER 6.24 or later\n");
        return;
    }

    crate::trace_console_msg("\n=== Mouse Modifiers ===\n\n");

    use crate::input::mouse_modifiers::actions::MouseModifierAction as ParsedAction;
    use crate::input::mouse_modifiers::core::AllModifierFlags;

    let mut total_found = 0;
    let mut contexts_with_modifiers = 0;
    let mut contexts_without_modifiers = Vec::new();

    for context in &*ALL_CONTEXTS {
        let mut context_actions = Vec::new();

        // Check all 16 possible modifier flag combinations (0-15)
        for (_flag_val, flag) in AllModifierFlags::new() {
            if let Some(action_str) = get_mouse_modifier(context, flag, medium_reaper) {
                // Only log if action has a value (not empty or "0")
                let trimmed = action_str.trim();
                if !trimmed.is_empty() && trimmed != "0" {
                    // Parse the action to get better display
                    let parsed_action = ParsedAction::parse(&action_str)
                        .unwrap_or_else(|_| ParsedAction::from(trimmed));
                    let flag_desc = flag.to_string();
                    context_actions.push((flag_desc, parsed_action));
                }
            }
        }

        // Log contexts that have modifiers
        if !context_actions.is_empty() {
            contexts_with_modifiers += 1;
            total_found += context_actions.len();

            // Use display name for better readability
            let display_name = crate::input::mouse_modifiers::contexts::get_display_name(context);
            crate::trace_console_msg(format!("{} ({})\n", display_name, context));

            // Group by action to reduce repetition
            let mut action_groups: std::collections::HashMap<String, Vec<String>> =
                std::collections::HashMap::new();
            for (flag_desc, parsed_action) in &context_actions {
                let action_str = parsed_action.to_string();
                action_groups
                    .entry(action_str.clone())
                    .or_default()
                    .push(flag_desc.clone());
            }

            // Sort actions by command ID for consistent output
            let mut sorted_actions: Vec<_> = action_groups.iter().collect();
            sorted_actions.sort_by_key(|(action, _)| {
                // Try to parse action ID for sorting
                ParsedAction::parse(action)
                    .ok()
                    .map(|a| a.command_id)
                    .unwrap_or(999999)
            });

            // Parse context string to get the context enum
            let context_enum =
                crate::input::mouse_modifiers::types::MouseModifierContext::from_reaper_string(
                    context,
                );

            // Log grouped actions with better formatting
            for (action_str, flags) in sorted_actions {
                // Parse the action to get the action ID
                let parsed_action = ParsedAction::parse(action_str)
                    .unwrap_or_else(|_| ParsedAction::from(action_str.as_str()));

                // Get display name using the new function
                let display_name = if let Some(ctx_enum) = &context_enum {
                    // Extract button input from context
                    let button_input = extract_button_input_from_context(ctx_enum);
                    crate::input::mouse_modifiers::behaviors::get_mouse_modifier_name(
                        ctx_enum,
                        button_input,
                        parsed_action.command_id,
                    )
                } else {
                    // Fallback if context parsing fails
                    parsed_action.original_string.clone()
                };

                // Format flags list more readably
                if flags.len() == 1 {
                    crate::trace_console_msg(format!(
                        "  {}: {} ({})\n",
                        flags[0], display_name, action_str
                    ));
                } else if flags.len() <= 3 {
                    // Small groups: show all
                    let flags_str = flags.join(", ");
                    crate::trace_console_msg(format!(
                        "  {}: {} ({})\n",
                        flags_str, display_name, action_str
                    ));
                } else {
                    // Large groups: show first, count, last
                    let flags_str = format!("{} ({} total)", flags[0], flags.len());
                    crate::trace_console_msg(format!(
                        "  {}: {} ({})\n",
                        flags_str, display_name, action_str
                    ));
                }
            }

            crate::trace_console_msg("\n");
        } else {
            // Track contexts without any modifiers
            let display_name = crate::input::mouse_modifiers::contexts::get_display_name(context);
            contexts_without_modifiers.push((display_name, context));
        }
    }

    // Show summary
    crate::trace_console_msg("=== Summary ===\n");
    crate::trace_console_msg(format!(
        "Found {} modifier assignments across {} contexts\n",
        total_found, contexts_with_modifiers
    ));

    // Show unmapped contexts if any
    if !contexts_without_modifiers.is_empty() {
        crate::trace_console_msg("\nUnmapped contexts (no modifiers assigned):\n");
        for (display_name, context) in &contexts_without_modifiers {
            crate::trace_console_msg(format!("  {} ({})\n", display_name, context));
        }
    }

    crate::trace_console_msg("\n=== End Mouse Modifiers ===\n\n");
}

/// Extract the button input from a context enum
/// This is a helper to determine which MouseButtonInput variant to use
fn extract_button_input_from_context(
    context: &crate::input::mouse_modifiers::types::MouseModifierContext,
) -> MouseButtonInput {
    match context {
        crate::input::mouse_modifiers::types::MouseModifierContext::ArrangeView(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::ArrangeViewInteraction::Middle(input) => {
                    *input
                }
                crate::input::mouse_modifiers::types::ArrangeViewInteraction::Right(input) => {
                    *input
                }
            }
        }
        crate::input::mouse_modifiers::types::MouseModifierContext::AutomationItem(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::AutomationItemInteraction::Default(input) => {
                    *input
                }
                crate::input::mouse_modifiers::types::AutomationItemInteraction::DoubleClick(
                    input,
                ) => *input,
                _ => MouseButtonInput::LeftDrag, // Edge doesn't have a button input
            }
        }
        crate::input::mouse_modifiers::types::MouseModifierContext::Envelope(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::EnvelopeInteraction::ControlPanel(input) => {
                    *input
                }
                crate::input::mouse_modifiers::types::EnvelopeInteraction::Lane(input) => *input,
                crate::input::mouse_modifiers::types::EnvelopeInteraction::Point(input) => *input,
                crate::input::mouse_modifiers::types::EnvelopeInteraction::Segment(input) => *input,
            }
        }
        crate::input::mouse_modifiers::types::MouseModifierContext::FixedLane(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::FixedLaneInteraction::HeaderButton(input) => {
                    *input
                }
                crate::input::mouse_modifiers::types::FixedLaneInteraction::LinkedLane(input) => {
                    *input
                }
            }
        }
        crate::input::mouse_modifiers::types::MouseModifierContext::Midi(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::MidiInteraction::CcEvent(input) => *input,
                crate::input::mouse_modifiers::types::MidiInteraction::CcLane(input) => *input,
                crate::input::mouse_modifiers::types::MidiInteraction::CcSegment(input) => *input,
                crate::input::mouse_modifiers::types::MidiInteraction::Note(note_interaction) => {
                    match note_interaction {
                        crate::input::mouse_modifiers::types::NoteInteraction::Default(input) => {
                            *input
                        }
                        _ => MouseButtonInput::LeftDrag, // Edge doesn't have a button input
                    }
                }
                crate::input::mouse_modifiers::types::MidiInteraction::PianoRoll(input) => *input,
                crate::input::mouse_modifiers::types::MidiInteraction::Right(input) => *input,
                crate::input::mouse_modifiers::types::MidiInteraction::Ruler(input) => *input,
                _ => MouseButtonInput::LeftDrag, // EndPointer, MarkerLanes don't have button inputs
            }
        }
        crate::input::mouse_modifiers::types::MouseModifierContext::MediaItem(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::MediaItemInteraction::Default(input) => {
                    *input
                }
                crate::input::mouse_modifiers::types::MediaItemInteraction::Edge(
                    edge_interaction,
                ) => match edge_interaction {
                    crate::input::mouse_modifiers::types::EdgeInteraction::Default(input) => *input,
                    crate::input::mouse_modifiers::types::EdgeInteraction::DoubleClick(input) => {
                        *input
                    }
                },
                crate::input::mouse_modifiers::types::MediaItemInteraction::Fade(input) => *input,
                crate::input::mouse_modifiers::types::MediaItemInteraction::Lower(input) => *input,
                crate::input::mouse_modifiers::types::MediaItemInteraction::StretchMarker(
                    stretch_interaction,
                ) => {
                    match stretch_interaction {
                        crate::input::mouse_modifiers::types::StretchMarkerInteraction::Default(input) => *input,
                        crate::input::mouse_modifiers::types::StretchMarkerInteraction::DoubleClick(input) => *input,
                        _ => MouseButtonInput::LeftDrag, // Rate doesn't have a button input
                    }
                }
                crate::input::mouse_modifiers::types::MediaItemInteraction::Crossfade(input) => {
                    *input
                }
            }
        }
        crate::input::mouse_modifiers::types::MouseModifierContext::Mixer(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::MixerInteraction::ControlPanel(input) => {
                    *input
                }
            }
        }
        crate::input::mouse_modifiers::types::MouseModifierContext::Project(_) => {
            // Project interactions don't have button inputs, use LeftDrag as default
            MouseButtonInput::LeftDrag
        }
        crate::input::mouse_modifiers::types::MouseModifierContext::RazorEdit(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::RazorEditInteraction::Area(input) => *input,
                _ => MouseButtonInput::LeftDrag, // Edge, EnvelopeArea don't have button inputs
            }
        }
        crate::input::mouse_modifiers::types::MouseModifierContext::Ruler(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::RulerInteraction::Default(input) => *input,
            }
        }
        crate::input::mouse_modifiers::types::MouseModifierContext::Track(interaction) => {
            match interaction {
                crate::input::mouse_modifiers::types::TrackInteraction::Default(input) => *input,
                crate::input::mouse_modifiers::types::TrackInteraction::ControlPanel(input) => {
                    *input
                }
            }
        }
        crate::input::mouse_modifiers::types::MouseModifierContext::CursorHandle => {
            MouseButtonInput::LeftDrag // Cursor handle uses left drag by default
        }
    }
}
