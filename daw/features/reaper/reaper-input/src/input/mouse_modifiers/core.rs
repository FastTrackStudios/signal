//! Core mouse modifier API functions
//!
//! Wrappers around REAPER's GetMouseModifier and SetMouseModifier functions.

use reaper_config::mouse::ModifierIndex;
use reaper_medium::Reaper as MediumReaper;
use std::ffi::CString;

/// Mouse modifier flag bits
/// Each context has 16 possible modifier combinations (0-15)
///
/// Flag bits:
/// - Bit 0 (1): Shift
/// - Bit 1 (2): Control (Cmd on macOS)
/// - Bit 2 (4): Alt (Opt on macOS)
/// - Bit 3 (8): Win (Ctrl on macOS)
///
/// Combinations:
/// - 0: Default action (no modifiers)
/// - 1: Shift
/// - 2: Control/Cmd
/// - 3: Shift+Control/Cmd
/// - 4: Alt/Opt
/// - 5: Shift+Alt/Opt
/// - 6: Control/Cmd+Alt/Opt
/// - 7: Shift+Control/Cmd+Alt/Opt
/// - 8: Win/Ctrl
/// - 9: Shift+Win/Ctrl
/// - 10: Control/Cmd+Win/Ctrl
/// - 11: Shift+Control/Cmd+Win/Ctrl
/// - 12: Alt/Opt+Win/Ctrl
/// - 13: Shift+Alt/Opt+Win/Ctrl
/// - 14: Control/Cmd+Alt/Opt+Win/Ctrl
/// - 15: Shift+Control/Cmd+Alt/Opt+Win/Ctrl
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseModifierFlag {
    pub shift: bool,
    pub control: bool, // Cmd on macOS
    pub alt: bool,     // Opt on macOS
    pub win: bool,     // Ctrl on macOS
}

impl MouseModifierFlag {
    pub fn new(shift: bool, control: bool, alt: bool, win: bool) -> Self {
        Self {
            shift,
            control,
            alt,
            win,
        }
    }

    pub fn none() -> Self {
        Self {
            shift: false,
            control: false,
            alt: false,
            win: false,
        }
    }

    /// Convert to REAPER modifier flag (0-15)
    pub fn to_flag(self) -> i32 {
        let mut flag = 0;
        if self.shift {
            flag |= 1;
        }
        if self.control {
            flag |= 2;
        }
        if self.alt {
            flag |= 4;
        }
        if self.win {
            flag |= 8;
        }
        flag
    }

    /// Create from REAPER modifier flag (0-15)
    pub fn from_flag(flag: i32) -> Self {
        Self {
            shift: (flag & 1) != 0,
            control: (flag & 2) != 0,
            alt: (flag & 4) != 0,
            win: (flag & 8) != 0,
        }
    }

    /// Add 1024 bit for no-move/no-select flags
    pub fn with_options(&self, no_move: bool, no_select: bool) -> i32 {
        let mut flag = (*self).to_flag();
        flag |= 1024; // Enable options bit
        if no_move {
            flag |= 1 << 10;
        } // Bit 10 = no-move
        if no_select {
            flag |= 2 << 10;
        } // Bit 11 = no-select
        flag
    }
}

impl std::fmt::Display for MouseModifierFlag {
    /// Get human-readable description of modifier combination
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.shift && !self.control && !self.alt && !self.win {
            write!(f, "Default action")
        } else {
            let mut parts = Vec::new();
            if self.shift {
                parts.push("Shift");
            }
            if self.control {
                parts.push("Cmd");
            } // Cmd on macOS, Ctrl on Windows
            if self.alt {
                parts.push("Opt");
            } // Opt on macOS, Alt on Windows
            if self.win {
                parts.push("Ctrl");
            } // Ctrl on macOS, Win on Windows
            write!(f, "{}", parts.join("+"))
        }
    }
}

/// Iterator over all 16 possible modifier flag combinations
pub struct AllModifierFlags {
    current: i32,
}

impl AllModifierFlags {
    pub fn new() -> Self {
        Self { current: 0 }
    }
}

impl Iterator for AllModifierFlags {
    type Item = (i32, MouseModifierFlag);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current < 16 {
            let flag = MouseModifierFlag::from_flag(self.current);
            let result = Some((self.current, flag));
            self.current += 1;
            result
        } else {
            None
        }
    }
}

impl Default for AllModifierFlags {
    fn default() -> Self {
        Self::new()
    }
}

impl From<MouseModifierFlag> for ModifierIndex {
    fn from(f: MouseModifierFlag) -> Self {
        Self(f.to_flag() as u8)
    }
}

impl From<ModifierIndex> for MouseModifierFlag {
    fn from(m: ModifierIndex) -> Self {
        MouseModifierFlag::from_flag(m.0 as i32)
    }
}

/// Get mouse modifier assignment for a specific context and modifier flag
/// Returns the action string (command ID or custom action ID)
pub fn get_mouse_modifier(
    context: &str,
    modifier_flag: MouseModifierFlag,
    medium_reaper: &MediumReaper,
) -> Option<String> {
    let low_reaper = medium_reaper.low();

    // Check if GetMouseModifier is available
    low_reaper.pointers().GetMouseModifier?;

    let context_cstr = match CString::new(context) {
        Ok(cstr) => cstr,
        Err(_) => return None,
    };
    let flag = modifier_flag.to_flag();

    // Allocate buffer for action string (REAPER typically uses 512 bytes)
    // Use i8 buffer since REAPER expects *mut c_char
    let mut action_buf = vec![0i8; 512];

    unsafe {
        low_reaper.GetMouseModifier(
            context_cstr.as_ptr(),
            flag,
            action_buf.as_mut_ptr(),
            action_buf.len() as i32,
        );
    }

    // Convert C string to Rust String
    // Find null terminator
    let null_pos = action_buf
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(action_buf.len());

    // Convert to bytes (i8 to u8)
    let action_bytes: Vec<u8> = action_buf[..null_pos].iter().map(|&b| b as u8).collect();

    let action_str = match String::from_utf8(action_bytes) {
        Ok(s) => s,
        Err(_) => return None,
    };

    // Return None if empty (no assignment)
    // Note: REAPER may return empty string for unassigned modifiers
    let trimmed = action_str.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Set mouse modifier assignment for a specific context and modifier flag
/// action can be:
/// - Command ID number (as string)
/// - Custom action ID string
/// - Mouse modifier ID with " m" appended
/// - Command ID with " c" appended
/// - Text from mouse modifiers dialog (unlocalized)
/// - -1 to reset to default
pub fn set_mouse_modifier(
    context: &str,
    modifier_flag: MouseModifierFlag,
    action: &str,
    medium_reaper: &MediumReaper,
) -> Result<(), Box<dyn std::error::Error>> {
    let low_reaper = medium_reaper.low();

    // Check if SetMouseModifier is available
    if low_reaper.pointers().SetMouseModifier.is_none() {
        return Err("SetMouseModifier not available".into());
    }

    let context_cstr = CString::new(context).map_err(|_| "Invalid context string")?;
    let flag = modifier_flag.to_flag();

    // Create the action CString outside the if block to keep it alive
    // IMPORTANT: CString must stay alive until after SetMouseModifier is called!
    let action_cstring = if action == "-1" {
        None
    } else {
        Some(CString::new(action).map_err(|_| "Invalid action string")?)
    };

    let action_ptr = action_cstring
        .as_ref()
        .map(|s| s.as_ptr())
        .unwrap_or(std::ptr::null());

    unsafe {
        low_reaper.SetMouseModifier(context_cstr.as_ptr(), flag, action_ptr);
    }

    Ok(())
}

/// Reset all mouse modifiers for a context
pub fn reset_context(
    context: &str,
    medium_reaper: &MediumReaper,
) -> Result<(), Box<dyn std::error::Error>> {
    set_mouse_modifier(context, MouseModifierFlag::none(), "-1", medium_reaper)
}

/// Parse a modifier string like `""`, `<S->`, `<C->`, `<A->`, `<S-C->`, etc.
/// into a [`MouseModifierFlag`].
///
/// Each letter in `<X-Y-Z->` maps to:
/// - `S` → Shift
/// - `C` → Ctrl/Cmd
/// - `A` → Alt/Opt
/// - `W` → Win/Super
pub fn parse_modifier_flags(mods: &str) -> MouseModifierFlag {
    let shift = mods.contains('S');
    let ctrl = mods.contains('C');
    let alt = mods.contains('A');
    let win = mods.contains('W');
    MouseModifierFlag::new(shift, ctrl, alt, win)
}

/// Reset all mouse modifiers (all contexts)
pub fn reset_all(medium_reaper: &MediumReaper) -> Result<(), Box<dyn std::error::Error>> {
    let low_reaper = medium_reaper.low();

    // Check if SetMouseModifier is available
    if low_reaper.pointers().SetMouseModifier.is_none() {
        return Err("SetMouseModifier not available".into());
    }

    unsafe {
        low_reaper.SetMouseModifier(std::ptr::null(), -1, std::ptr::null());
    }
    Ok(())
}
