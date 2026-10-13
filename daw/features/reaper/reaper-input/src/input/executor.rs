//! Input Executor
//!
//! Executes commands by running the associated REAPER actions.

use crate::input::state::Command;
use helgoboss_midi::U7;
use reaper_high::Reaper;
use reaper_medium::{ActionValueChange, CommandId, ProjectContext, WindowContext};
use tracing::{debug, warn};

/// REAPER wheel actions treat relative value 1 as a tiny nudge. A physical
/// wheel notch usually arrives as delta 120, so scale that notch into a more
/// usable relative action step.
const WHEEL_RELATIVE_UNITS_PER_NOTCH: u16 = 8;

/// Execute a command
pub fn execute_command(command: &Command) -> Result<(), Box<dyn std::error::Error>> {
    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    debug!("Executing command: {:?}", command);

    // Execute each action in the sequence
    for (i, _action_type) in command.action_sequence.iter().enumerate() {
        let action_key = &command.action_keys[i];

        // Look up the command ID
        let cmd_id = if let Ok(numeric_id) = action_key.identifier.parse::<u32>() {
            CommandId::new(numeric_id)
        } else if let Some(named_id) =
            medium_reaper.named_command_lookup(action_key.identifier.as_str())
        {
            named_id
        } else {
            warn!("Could not find command: {}", action_key.identifier);
            continue;
        };

        // Execute with repetition if specified
        let repetitions = action_key.repetition_count.unwrap_or(1);
        for _ in 0..repetitions {
            medium_reaper.main_on_command_ex(cmd_id, 1, ProjectContext::CurrentProject);
        }

        debug!(
            "Executed action: {} ({} times)",
            action_key.identifier, repetitions
        );
    }

    Ok(())
}

/// Execute a composed action sequence
/// This handles special composition logic (e.g., timeline_operator + timeline_motion)
pub fn execute_composed_command(command: &Command) -> Result<(), Box<dyn std::error::Error>> {
    // For now, just execute normally
    // TODO: Add composition logic similar to reaper-keys' action_sequence.lua
    execute_command(command)
}

/// Execute an action by its identifier (action name or numeric ID)
pub fn execute_action(action_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    // Look up command ID
    let cmd_id = if let Ok(numeric_id) = action_id.parse::<u32>() {
        CommandId::new(numeric_id)
    } else if let Some(named_id) = medium_reaper.named_command_lookup(action_id) {
        named_id
    } else {
        return Err(format!("Could not find command: {}", action_id).into());
    };

    // Execute the action
    medium_reaper.main_on_command_ex(cmd_id, 1, ProjectContext::CurrentProject);
    debug!("Executed action: {}", action_id);

    Ok(())
}

/// Execute an action for wheel/relative input
///
/// This executes an action that responds to wheel/relative input using
/// REAPER's KBD_OnMainActionEx API with relative mode encoding.
///
/// # Arguments
/// * `action_id` - Action name or numeric ID
/// * `delta` - Wheel delta (positive = up, negative = down)
///
/// # Safety
/// This function uses unsafe code to call the low-level REAPER API.
pub fn execute_wheel_action(action_id: &str, delta: i16) -> Result<(), Box<dyn std::error::Error>> {
    if action_id.contains('+') || action_id.contains(',') {
        for action in action_id
            .split(['+', ','])
            .map(str::trim)
            .filter(|action| !action.is_empty())
        {
            execute_wheel_action(action, delta)?;
        }
        return Ok(());
    }

    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    // Look up command ID
    let cmd_id = if let Ok(numeric_id) = action_id.parse::<u32>() {
        CommandId::new(numeric_id)
    } else if let Some(named_id) = medium_reaper.named_command_lookup(action_id) {
        named_id
    } else {
        return Err(format!("Could not find command: {}", action_id).into());
    };

    // Create the ActionValueChange using Relative1 mode:
    // - 127 → -1 (scroll down)
    // - 1 → +1 (scroll up)
    let notches = wheel_relative_units(delta);
    let value_change = if delta > 0 {
        ActionValueChange::Relative1(U7::new(notches))
    } else {
        // For Relative1: values > 64 are negative (128 - value = magnitude)
        ActionValueChange::Relative1(U7::new(128 - notches))
    };

    // Execute using kbd_on_main_action_ex which properly handles relative values
    // SAFETY: We're passing valid command ID and project context
    unsafe {
        medium_reaper.kbd_on_main_action_ex(
            cmd_id,
            value_change,
            WindowContext::NoWindow,
            ProjectContext::CurrentProject,
        );
    }

    debug!(
        "Executed wheel action: {} (delta={}, value_change={:?})",
        action_id, delta, value_change
    );

    Ok(())
}

/// Translate a relative ("MIDI relative/mousewheel") zoom/scroll action into a
/// pair of discrete MIDI Editor commands `(positive_delta, negative_delta)`.
///
/// REAPER's relative mousewheel actions do not fire when invoked
/// programmatically through `kbd_RunCommandThroughHooks` (they expect the
/// editor to feed them the live wheel value), so we drive the equivalent
/// discrete commands per wheel notch via `MIDIEditor_OnCommand` instead.
///
/// `positive_delta` runs on wheel-up (delta > 0), `negative_delta` on
/// wheel-down. The "reversed" action variants invert the mapping so they
/// keep matching the arrange-view zoom feel.
fn midi_wheel_discrete(action_id: &str) -> Option<(i32, i32)> {
    Some(match action_id {
        // --- Scroll ---
        "40432" => (40138, 40139), // scroll vertically: up / down
        "40661" => (40139, 40138), // scroll vertically reversed
        "40433" => (40141, 40140), // scroll horizontally: right / left
        "40660" => (40140, 40141), // scroll horizontally reversed
        // --- Zoom (normal: up = in, down = out) ---
        "40430" => (40111, 40112), // zoom vertically: in / out
        "40431" => (1012, 1011),   // zoom horizontally: in / out
        // --- Zoom (reversed: up = out, down = in) ---
        "40663" => (40112, 40111), // zoom vertically reversed
        "40662" => (1011, 1012),   // zoom horizontally reversed
        // --- CC lane zoom ---
        "42435" => (42435, 42436), // CC lane zoom in / out
        "42436" => (42436, 42435),
        _ => return None,
    })
}

/// Execute a wheel/relative input action in the MIDI Editor.
///
/// Relative mousewheel actions are translated to discrete MIDI Editor
/// commands (see [`midi_wheel_discrete`]) and run once per wheel notch via
/// `MIDIEditor_OnCommand`, since the relative actions don't fire when invoked
/// programmatically. Non-wheel actions fall back to a single dispatch.
///
/// # Arguments
/// * `action_id` - Action name or numeric ID (from MIDI Editor section)
/// * `delta` - Wheel delta (positive = up, negative = down)
pub fn execute_midi_editor_wheel_action(
    action_id: &str,
    delta: i16,
) -> Result<(), Box<dyn std::error::Error>> {
    // Composite actions (e.g. "40663+40662" to zoom vertically and
    // horizontally at once) — run each part in turn, matching the arrange
    // `execute_wheel_action` behaviour.
    if action_id.contains('+') || action_id.contains(',') {
        for action in action_id
            .split(['+', ','])
            .map(str::trim)
            .filter(|action| !action.is_empty())
        {
            execute_midi_editor_wheel_action(action, delta)?;
        }
        return Ok(());
    }

    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();
    let low_reaper = medium_reaper.low();

    // Get the active MIDI editor window
    let Some(midi_editor_hwnd) = medium_reaper.midi_editor_get_active() else {
        return Err("No active MIDI editor".into());
    };
    let hwnd = midi_editor_hwnd.as_ptr();

    // One discrete step per wheel notch (delta is usually ±120 = one notch);
    // cap the repeat so a fast flick can't fire a runaway number of steps.
    let notches = (u16::try_from(delta.saturating_abs())
        .unwrap_or(u16::MAX)
        .div_ceil(120))
    .clamp(1, 10);

    // Relative wheel action → discrete command for this wheel direction.
    if let Some((pos, neg)) = midi_wheel_discrete(action_id) {
        let cmd = if delta > 0 { pos } else { neg };
        for _ in 0..notches {
            unsafe { low_reaper.MIDIEditor_OnCommand(hwnd, cmd) };
        }
        debug!(
            "Executed MIDI editor wheel action: {} -> {} x{} (delta={})",
            action_id, cmd, notches, delta
        );
        return Ok(());
    }

    // Not a known relative wheel action — dispatch it once as-is.
    let cmd_id = if let Ok(numeric_id) = action_id.parse::<i32>() {
        numeric_id
    } else if let Some(named_id) = medium_reaper.named_command_lookup(action_id) {
        named_id.get() as i32
    } else {
        return Err(format!("Could not find command: {}", action_id).into());
    };
    unsafe { low_reaper.MIDIEditor_OnCommand(hwnd, cmd_id) };
    debug!(
        "Executed MIDI editor action: {} (delta={}, non-relative)",
        action_id, delta
    );
    Ok(())
}

fn wheel_relative_units(delta: i16) -> u8 {
    let abs_delta = u16::try_from(delta.saturating_abs()).unwrap_or(u16::MAX);
    let raw_notches = abs_delta.div_ceil(120).max(1);
    let scaled = raw_notches.saturating_mul(WHEEL_RELATIVE_UNITS_PER_NOTCH);
    scaled.min(63) as u8
}
