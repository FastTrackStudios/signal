//! Typesafe representation of REAPER mouse modifier actions
//!
//! Actions are stored as strings like "1 m", "2 m", "13 m", "1 e", etc.
//! Format: "{command_id} {section}"
//! - command_id: Numeric action command ID
//! - section: Single character indicating section ('m' = main, 'e' = MIDI editor, 'x' = media explorer, etc.)

use std::fmt;

/// REAPER action section identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionSection {
    /// Main section (default)
    Main,
    /// MIDI editor section
    MidiEditor,
    /// Media explorer section
    MediaExplorer,
    /// Unknown or custom section
    Other(char),
}

impl ActionSection {
    /// Parse section from character
    pub fn from_char(c: char) -> Self {
        match c {
            'm' => ActionSection::Main,
            'e' => ActionSection::MidiEditor,
            'x' => ActionSection::MediaExplorer,
            other => ActionSection::Other(other),
        }
    }

    /// Get character representation
    pub fn to_char(self) -> char {
        match self {
            ActionSection::Main => 'm',
            ActionSection::MidiEditor => 'e',
            ActionSection::MediaExplorer => 'x',
            ActionSection::Other(c) => c,
        }
    }

    /// Get display name
    pub fn display_name(self) -> &'static str {
        match self {
            ActionSection::Main => "Main",
            ActionSection::MidiEditor => "MIDI Editor",
            ActionSection::MediaExplorer => "Media Explorer",
            ActionSection::Other(_c) => {
                // Return a static string for unknown sections
                // Note: This is a limitation - we can't return &str for arbitrary chars
                // In practice, we'll format it differently
                "Unknown"
            }
        }
    }
}

impl fmt::Display for ActionSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionSection::Main => write!(f, "Main"),
            ActionSection::MidiEditor => write!(f, "MIDI Editor"),
            ActionSection::MediaExplorer => write!(f, "Media Explorer"),
            ActionSection::Other(c) => write!(f, "Section '{}'", c),
        }
    }
}

/// Typesafe representation of a REAPER mouse modifier action
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MouseModifierAction {
    /// The action command ID
    pub command_id: u32,
    /// The section this action belongs to
    pub section: ActionSection,
    /// Original action string (for cases where parsing fails or custom formats)
    pub original_string: String,
}

impl MouseModifierAction {
    /// Create a new action from command ID and section
    pub fn new(command_id: u32, section: ActionSection) -> Self {
        let original_string = format!("{} {}", command_id, section.to_char());
        Self {
            command_id,
            section,
            original_string,
        }
    }

    /// Parse action string (e.g., "1 m", "13 m", "2 e")
    pub fn parse(action_str: &str) -> Result<Self, String> {
        let trimmed = action_str.trim();

        // Handle empty or reset values
        if trimmed.is_empty() || trimmed == "0" || trimmed == "-1" {
            return Err("Empty or reset action".to_string());
        }

        // Try to parse as "ID section" format
        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        if parts.len() >= 2 {
            // Format: "ID section" (e.g., "1 m", "13 m")
            if let Ok(command_id) = parts[0].parse::<u32>()
                && let Some(section_char) = parts[1].chars().next()
            {
                let section = ActionSection::from_char(section_char);
                return Ok(Self {
                    command_id,
                    section,
                    original_string: trimmed.to_string(),
                });
            }
        } else if parts.len() == 1 {
            // Try to parse as just a number (assume main section)
            if let Ok(command_id) = parts[0].parse::<u32>()
                && command_id < 1000
            {
                // Numbers < 1000 default to main section per REAPER convention
                return Ok(Self {
                    command_id,
                    section: ActionSection::Main,
                    original_string: format!("{} m", command_id),
                });
            }
        }

        // If parsing fails, store as-is
        Ok(Self {
            command_id: 0,
            section: ActionSection::Main,
            original_string: trimmed.to_string(),
        })
    }

    /// Check if this is an empty/reset action
    pub fn is_empty(&self) -> bool {
        self.original_string.trim().is_empty()
            || self.original_string == "0"
            || self.original_string == "-1"
    }
}

impl From<&str> for MouseModifierAction {
    fn from(s: &str) -> Self {
        Self::parse(s).unwrap_or_else(|_| Self {
            command_id: 0,
            section: ActionSection::Main,
            original_string: s.to_string(),
        })
    }
}

impl From<String> for MouseModifierAction {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl fmt::Display for MouseModifierAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // For now, just show the original string
        // TODO: Look up action name from REAPER API if available
        write!(f, "{}", self.original_string)
    }
}

/// Try to get a human-readable name for an action using REAPER's API
/// Note: This only works for custom actions with named command IDs.
/// For internal mouse modifier actions, use `get_mouse_modifier_name` instead
/// which requires the context and button input.
pub fn get_action_display_name(
    action: &MouseModifierAction,
    medium_reaper: &reaper_medium::Reaper,
) -> String {
    // Try REAPER's API lookup (for custom actions that might have named IDs)
    if action.command_id > 0 {
        let cmd_id = reaper_medium::CommandId::new(action.command_id);
        if let Some(name_str) =
            medium_reaper.reverse_named_command_lookup(cmd_id, |s| s.to_string())
        {
            // If we got a name from API, return it (remove leading underscore if present)
            let name = if name_str.starts_with('_') {
                name_str[1..].to_string()
            } else {
                name_str
            };
            // Only use API result if it looks like a real action name (not just a number)
            if !name.chars().all(|c| c.is_ascii_digit() || c == ' ') {
                return name;
            }
        }
    }

    // If API lookup fails, return the action ID string
    // For proper display names, use get_mouse_modifier_name with context
    action.original_string.clone()
}
