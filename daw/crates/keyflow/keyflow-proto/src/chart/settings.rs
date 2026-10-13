//! Chart Settings
//!
//! Configuration options for chart parsing and display

use crate::chord::{LilySyntax, PushPullBase};
use facet::Facet;
use std::collections::HashMap;

/// Chart configuration settings
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ChartSettings {
    /// Internal settings storage
    settings: HashMap<ChartSetting, SettingValue>,
    /// Default push/pull base (standard, triplet, or tuplet)
    pub push_mode: PushPullBase,
    /// Swing ratio for MIDI playback (0.5 = straight, 0.6667 = triplet)
    pub swing: Option<f64>,
}

/// Available chart settings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum ChartSetting {
    /// Automatically group repeated phrases into 4-bar units with repeat signs
    SmartRepeats,
    /// Default push/pull mode (standard, triplet, or tuplet number)
    PushMode,
    /// Automatically fill whole/half notes with quarter note slashes
    /// When enabled (default), a whole note chord becomes 4 quarter slashes,
    /// a half note becomes 2 quarter slashes. This is standard for master rhythm charts.
    AutoRhythmSlashes,
    /// Whether push/pull notation alters the rhythm display
    /// When enabled (default), pushed chords create triplet/syncopated notation.
    /// When disabled, pushed chords show apostrophe markers on chord symbols instead.
    PushAltersRhythm,
    /// Swing ratio for MIDI playback (0.5 = straight, 0.6667 = triplet swing)
    Swing,
    /// Chord memory: a bare root recalls the quality its degree last had
    /// (`4` after `4maj7` is `4maj7`). OFF by default — a chord is what is
    /// written; `/CHORD_MEMORY=true` turns it on for a chart.
    ChordMemory,
}

/// Setting value types
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum SettingValue {
    Bool(bool),
    String(String),
    Number(i32),
}

impl ChartSettings {
    /// Create new default settings
    pub fn new() -> Self {
        let mut settings = HashMap::new();

        // Set defaults
        settings.insert(ChartSetting::SmartRepeats, SettingValue::Bool(false));
        settings.insert(ChartSetting::AutoRhythmSlashes, SettingValue::Bool(true)); // ON by default
        settings.insert(ChartSetting::PushAltersRhythm, SettingValue::Bool(true)); // ON by default
        settings.insert(ChartSetting::ChordMemory, SettingValue::Bool(false));

        Self {
            settings,
            push_mode: PushPullBase::Standard,
            swing: None,
        }
    }

    /// Parse a setting line (e.g., "/SMART_REPEATS=true" or "/push 4")
    ///
    /// Supports two syntaxes:
    /// - `/SETTING=value` - standard key=value format
    /// - `/push 4` - space-separated format for push mode specifically
    pub fn parse_setting_line(&mut self, line: &str) -> Result<(), String> {
        // Remove leading slash and trim
        let line = line.trim().trim_start_matches('/').trim();

        // Try splitting by '=' first (standard format)
        let (key, value): (String, String) = if let Some(eq_pos) = line.find('=') {
            let (k, v) = line.split_at(eq_pos);
            (k.trim().to_uppercase(), v[1..].trim().to_string())
        } else {
            // No '=' found - try space-separated format for PUSH/SWING
            let parts: Vec<&str> = line.split_whitespace().collect();
            let upper = parts.first().map(|s| s.to_uppercase());
            if parts.len() >= 2 && matches!(upper.as_deref(), Some("PUSH") | Some("SWING")) {
                (upper.unwrap(), parts[1..].join(" "))
            } else {
                return Err(format!(
                    "Invalid setting format: '{}'. Expected /SETTING=value or /push <mode>",
                    line
                ));
            }
        };

        let value = value.as_str();

        match key.as_str() {
            "SMART_REPEATS" => {
                let bool_value = Self::parse_bool(value)?;
                self.set(ChartSetting::SmartRepeats, SettingValue::Bool(bool_value));
                Ok(())
            }
            "PUSH" => {
                self.push_mode = Self::parse_push_mode(value)?;
                Ok(())
            }
            "AUTO_RHYTHM_SLASHES" | "AUTORHYTHMSLASHES" | "AUTO_SLASHES" => {
                let bool_value = Self::parse_bool(value)?;
                self.set(
                    ChartSetting::AutoRhythmSlashes,
                    SettingValue::Bool(bool_value),
                );
                Ok(())
            }
            "CHORD_MEMORY" | "CHORD-MEMORY" | "CHORDMEMORY" => {
                let bool_value = Self::parse_bool(value)?;
                self.set(ChartSetting::ChordMemory, SettingValue::Bool(bool_value));
                Ok(())
            }
            "PUSH_ALTERS_RHYTHM" | "PUSHALTERSRHYTHM" => {
                let bool_value = Self::parse_bool(value)?;
                self.set(
                    ChartSetting::PushAltersRhythm,
                    SettingValue::Bool(bool_value),
                );
                Ok(())
            }
            "SWING" => {
                let swing_value = match value.to_lowercase().as_str() {
                    "triplet" => 0.6667,
                    "straight" | "none" => 0.5,
                    _ => value.parse::<f64>().map_err(|_| {
                        format!(
                            "Invalid swing value: '{}'. Expected 'triplet', 'straight', or a number",
                            value
                        )
                    })?,
                };
                self.swing = Some(swing_value);
                Ok(())
            }
            _ => Err(format!("Unknown setting: '{}'", key)),
        }
    }

    /// Parse push mode value: "standard", "triplet", a tuplet number, or duration syntax
    ///
    /// Duration syntax examples:
    /// - "4" → quarter note push
    /// - "8" → eighth note push
    /// - "8t" → triplet eighth push
    /// - "4." → dotted quarter push
    /// - "16" → sixteenth note push
    fn parse_push_mode(value: &str) -> Result<PushPullBase, String> {
        let value_lower = value.to_lowercase();
        match value_lower.as_str() {
            "standard" | "normal" | "binary" => Ok(PushPullBase::Standard),
            "triplet" => Ok(PushPullBase::Triplet),
            _ => {
                // Check for duration syntax: number followed by optional 't' (triplet) or '.' (dotted)
                let trimmed = value.trim();
                let (num_part, suffix) = if let Some(stripped) = trimmed.strip_suffix('t') {
                    (stripped, Some('t'))
                } else if let Some(stripped) = trimmed.strip_suffix('.') {
                    (stripped, Some('.'))
                } else {
                    (trimmed, None)
                };

                // Try to parse as a LilySyntax duration first (1, 2, 4, 8, 16, 32)
                if let Some(duration) = LilySyntax::from_number(num_part) {
                    let dotted = suffix == Some('.');
                    let triplet = suffix == Some('t');
                    return Ok(PushPullBase::Duration {
                        duration,
                        dotted,
                        triplet,
                    });
                }

                // Try to parse as a tuplet number (3, 5, 7, 9, etc.)
                // Only if there's no suffix (otherwise it would have matched duration)
                if suffix.is_none()
                    && let Ok(n) = num_part.parse::<u8>()
                {
                    if n == 3 {
                        return Ok(PushPullBase::Triplet);
                    }
                    if n >= 4 {
                        // Numbers >= 4 that aren't valid LilySyntax (handled above) are tuplets
                        return Ok(PushPullBase::Tuplet(n));
                    }
                }

                Err(format!(
                    "Invalid push mode: '{}'. Expected 'standard', 'triplet', duration (4, 8t, 16.), or tuplet number",
                    value
                ))
            }
        }
    }

    /// Parse a boolean value from string
    fn parse_bool(value: &str) -> Result<bool, String> {
        match value.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(format!(
                "Invalid boolean value: '{}'. Expected true/false",
                value
            )),
        }
    }

    /// Set a setting value
    pub fn set(&mut self, setting: ChartSetting, value: SettingValue) {
        self.settings.insert(setting, value);
    }

    /// Get a setting value
    pub fn get(&self, setting: ChartSetting) -> Option<&SettingValue> {
        self.settings.get(&setting)
    }

    /// Get a boolean setting (with default fallback)
    pub fn get_bool(&self, setting: ChartSetting) -> bool {
        match self.settings.get(&setting) {
            Some(SettingValue::Bool(b)) => *b,
            _ => false,
        }
    }

    /// Get a string setting (with default fallback)
    pub fn get_string(&self, setting: ChartSetting) -> Option<String> {
        match self.settings.get(&setting) {
            Some(SettingValue::String(s)) => Some(s.clone()),
            _ => None,
        }
    }

    /// Get a number setting (with default fallback)
    pub fn get_number(&self, setting: ChartSetting) -> Option<i32> {
        match self.settings.get(&setting) {
            Some(SettingValue::Number(n)) => Some(*n),
            _ => None,
        }
    }

    /// Check if smart repeats is enabled
    pub fn smart_repeats(&self) -> bool {
        self.get_bool(ChartSetting::SmartRepeats)
    }

    /// Whether chord memory is on (default: off). See [`ChartSetting::ChordMemory`].
    pub fn chord_memory(&self) -> bool {
        self.get_bool(ChartSetting::ChordMemory)
    }

    /// Check if auto rhythm slashes is enabled (default: true)
    ///
    /// When enabled, whole notes and half notes in rhythm charts are automatically
    /// expanded to quarter note slashes. For example:
    /// - A whole note chord becomes 4 quarter slashes
    /// - A half note chord becomes 2 quarter slashes
    ///
    /// This is standard notation for master rhythm charts.
    pub fn auto_rhythm_slashes(&self) -> bool {
        // Default to true if not explicitly set
        match self.settings.get(&ChartSetting::AutoRhythmSlashes) {
            Some(SettingValue::Bool(b)) => *b,
            _ => true, // Default ON
        }
    }

    /// Check if push alters rhythm is enabled (default: true)
    ///
    /// When enabled, pushed chords create triplet/syncopated rhythm notation
    /// showing exactly when the chord should be played.
    ///
    /// When disabled, pushed chords show simple apostrophe markers on the
    /// chord symbols (`'C` for push, `C'` for pull) in a contrasting color.
    /// The rhythm notation remains on-beat for simpler reading.
    pub fn push_alters_rhythm(&self) -> bool {
        // Default to true if not explicitly set
        match self.settings.get(&ChartSetting::PushAltersRhythm) {
            Some(SettingValue::Bool(b)) => *b,
            _ => true, // Default ON
        }
    }
}

impl Default for ChartSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartSettings {
    /// Create a checkpoint of the current settings state.
    /// This is used for section-scoped settings - settings declared inside a section
    /// are temporary and reset after the section ends.
    pub fn checkpoint(&self) -> ChartSettingsCheckpoint {
        ChartSettingsCheckpoint {
            settings: self.settings.clone(),
            push_mode: self.push_mode,
            swing: self.swing,
        }
    }

    /// Restore settings from a checkpoint.
    /// Any settings changed since the checkpoint was created are reverted.
    pub fn restore(&mut self, checkpoint: ChartSettingsCheckpoint) {
        self.settings = checkpoint.settings;
        self.push_mode = checkpoint.push_mode;
        self.swing = checkpoint.swing;
    }
}

/// A checkpoint of chart settings state.
/// Used to implement section-scoped settings that reset after the section ends.
#[derive(Debug, Clone)]
pub struct ChartSettingsCheckpoint {
    settings: HashMap<ChartSetting, SettingValue>,
    push_mode: PushPullBase,
    swing: Option<f64>,
}

impl ChartSetting {
    /// Get the display name for this setting
    pub fn name(&self) -> &'static str {
        match self {
            ChartSetting::SmartRepeats => "SMART_REPEATS",
            ChartSetting::PushMode => "PUSH",
            ChartSetting::AutoRhythmSlashes => "AUTO_RHYTHM_SLASHES",
            ChartSetting::PushAltersRhythm => "PUSH_ALTERS_RHYTHM",
            ChartSetting::Swing => "SWING",
            ChartSetting::ChordMemory => "CHORD_MEMORY",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = ChartSettings::new();
        assert!(!settings.smart_repeats());
    }

    #[test]
    fn chord_memory_is_off_until_a_chart_turns_it_on() {
        let mut settings = ChartSettings::new();
        assert!(!settings.chord_memory());
        settings.parse_setting_line("/CHORD_MEMORY=true").unwrap();
        assert!(settings.chord_memory());
        settings
            .parse_setting_line("/chord-memory = false")
            .unwrap();
        assert!(!settings.chord_memory());
    }

    #[test]
    fn test_parse_smart_repeats_true() {
        let mut settings = ChartSettings::new();
        settings.parse_setting_line("/SMART_REPEATS=true").unwrap();
        assert!(settings.smart_repeats());
    }

    #[test]
    fn test_parse_smart_repeats_false() {
        let mut settings = ChartSettings::new();
        settings.parse_setting_line("/SMART_REPEATS=false").unwrap();
        assert!(!settings.smart_repeats());
    }

    #[test]
    fn test_parse_bool_variations() {
        let mut settings = ChartSettings::new();

        // Test various true values
        settings.parse_setting_line("/SMART_REPEATS=1").unwrap();
        assert!(settings.smart_repeats());

        settings.parse_setting_line("/SMART_REPEATS=yes").unwrap();
        assert!(settings.smart_repeats());

        settings.parse_setting_line("/SMART_REPEATS=on").unwrap();
        assert!(settings.smart_repeats());

        // Test various false values
        settings.parse_setting_line("/SMART_REPEATS=0").unwrap();
        assert!(!settings.smart_repeats());

        settings.parse_setting_line("/SMART_REPEATS=no").unwrap();
        assert!(!settings.smart_repeats());

        settings.parse_setting_line("/SMART_REPEATS=off").unwrap();
        assert!(!settings.smart_repeats());
    }

    #[test]
    fn test_parse_invalid_setting() {
        let mut settings = ChartSettings::new();
        let result = settings.parse_setting_line("/UNKNOWN_SETTING=true");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_format() {
        let mut settings = ChartSettings::new();
        let result = settings.parse_setting_line("/SMART_REPEATS");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_bool() {
        let mut settings = ChartSettings::new();
        let result = settings.parse_setting_line("/SMART_REPEATS=maybe");
        assert!(result.is_err());
    }

    #[test]
    fn test_case_insensitive_setting_name() {
        let mut settings = ChartSettings::new();
        settings.parse_setting_line("/smart_repeats=true").unwrap();
        assert!(settings.smart_repeats());
    }

    #[test]
    fn test_whitespace_handling() {
        let mut settings = ChartSettings::new();
        settings
            .parse_setting_line("  /  SMART_REPEATS  =  true  ")
            .unwrap();
        assert!(settings.smart_repeats());
    }

    #[test]
    fn test_push_mode_standard() {
        let mut settings = ChartSettings::new();
        settings.parse_setting_line("/push=standard").unwrap();
        assert!(matches!(settings.push_mode, PushPullBase::Standard));
    }

    #[test]
    fn test_push_mode_triplet() {
        let mut settings = ChartSettings::new();
        settings.parse_setting_line("/push=triplet").unwrap();
        assert!(matches!(settings.push_mode, PushPullBase::Triplet));
    }

    #[test]
    fn test_push_mode_duration_quarter() {
        let mut settings = ChartSettings::new();
        settings.parse_setting_line("/push=4").unwrap();
        match settings.push_mode {
            PushPullBase::Duration {
                duration,
                dotted,
                triplet,
            } => {
                assert_eq!(duration, LilySyntax::Quarter);
                assert!(!dotted);
                assert!(!triplet);
            }
            _ => panic!("Expected Duration variant"),
        }
    }

    #[test]
    fn test_push_mode_duration_eighth_triplet() {
        let mut settings = ChartSettings::new();
        settings.parse_setting_line("/push=8t").unwrap();
        match settings.push_mode {
            PushPullBase::Duration {
                duration,
                dotted,
                triplet,
            } => {
                assert_eq!(duration, LilySyntax::Eighth);
                assert!(!dotted);
                assert!(triplet);
            }
            _ => panic!("Expected Duration variant"),
        }
    }

    #[test]
    fn test_push_mode_duration_dotted() {
        let mut settings = ChartSettings::new();
        settings.parse_setting_line("/push=4.").unwrap();
        match settings.push_mode {
            PushPullBase::Duration {
                duration,
                dotted,
                triplet,
            } => {
                assert_eq!(duration, LilySyntax::Quarter);
                assert!(dotted);
                assert!(!triplet);
            }
            _ => panic!("Expected Duration variant"),
        }
    }

    #[test]
    fn test_push_mode_sixteenth() {
        let mut settings = ChartSettings::new();
        settings.parse_setting_line("/push=16").unwrap();
        match settings.push_mode {
            PushPullBase::Duration {
                duration,
                dotted,
                triplet,
            } => {
                assert_eq!(duration, LilySyntax::Sixteenth);
                assert!(!dotted);
                assert!(!triplet);
            }
            _ => panic!("Expected Duration variant"),
        }
    }

    #[test]
    fn test_push_mode_space_separated() {
        // Test that "/push 4" works without equals sign
        let mut settings = ChartSettings::new();
        settings.parse_setting_line("/push 4").unwrap();
        match settings.push_mode {
            PushPullBase::Duration {
                duration,
                dotted,
                triplet,
            } => {
                assert_eq!(duration, LilySyntax::Quarter);
                assert!(!dotted);
                assert!(!triplet);
            }
            _ => panic!("Expected Duration variant"),
        }

        // Also test with triplet modifier
        settings.parse_setting_line("/push 8t").unwrap();
        match settings.push_mode {
            PushPullBase::Duration {
                duration,
                dotted,
                triplet,
            } => {
                assert_eq!(duration, LilySyntax::Eighth);
                assert!(!dotted);
                assert!(triplet);
            }
            _ => panic!("Expected Duration variant with triplet"),
        }
    }

    #[test]
    fn test_settings_checkpoint_restore() {
        let mut settings = ChartSettings::new();

        // Set to triplet mode
        settings.parse_setting_line("/push=triplet").unwrap();
        settings.parse_setting_line("/smart_repeats=true").unwrap();

        // Create checkpoint
        let checkpoint = settings.checkpoint();

        // Change settings
        settings.parse_setting_line("/push=standard").unwrap();
        settings.parse_setting_line("/smart_repeats=false").unwrap();

        // Verify changes took effect
        assert!(matches!(settings.push_mode, PushPullBase::Standard));
        assert!(!settings.smart_repeats());

        // Restore from checkpoint
        settings.restore(checkpoint);

        // Verify restoration
        assert!(matches!(settings.push_mode, PushPullBase::Triplet));
        assert!(settings.smart_repeats());
    }
}
