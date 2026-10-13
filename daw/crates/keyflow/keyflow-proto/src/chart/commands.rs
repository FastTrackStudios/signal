//! Chart Commands
//!
//! Special commands that can be applied to chords, melodies, or rhythms
//! Commands can be specified with slash syntax (/fermata) or shorthand (->)

use facet::Facet;
use std::fmt;

/// Commands that can be applied to musical elements
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum Command {
    /// Fermata - hold the note/chord longer
    Fermata,

    /// Accent - emphasize the note/chord on the downbeat
    /// Syntax: '>C (accent after push marker = accent on beat 1)
    Accent,

    /// Accent on the pushed/anticipation beat
    /// Syntax: >'C (accent before push marker = accent on the push, e.g., beat 4.66)
    /// This renders the accent at the spillback position in the previous measure
    AccentOnPush,

    /// Staccato - short, detached chord hit
    /// Syntax: .C (staccato chord), >.'C (accented staccato push)
    /// Renders a staccato dot (·) above the rhythm slash notehead
    Staccato,

    /// Stop sign rendered BEFORE the chord (octagonal shape)
    /// Syntax: !STOP C — "stop, then hit C"
    /// Indicates a full stop / cutoff point before the chord
    Stop,

    /// Stop sign rendered AFTER the chord (octagonal shape)
    /// Syntax: C !STOP — "hit C, then stop"
    /// Indicates a full stop / cutoff point after the chord
    StopAfter,

    /// Stop Groove rendered BEFORE the chord (circular shape)
    /// Syntax: !STOPGROOVE C — "groove stop, then hit C"
    StopGroove,

    /// Stop Groove rendered AFTER the chord (circular shape)
    /// Syntax: C !STOPGROOVE — "hit C, then groove stop"
    StopGrooveAfter,
}

impl Command {
    /// Parse a command from a slash notation (e.g., "/fermata", "/accent")
    pub fn parse_slash(text: &str) -> Option<Self> {
        let text = text.trim().trim_start_matches('/').trim().to_lowercase();

        match text.as_str() {
            "fermata" => Some(Command::Fermata),
            "accent" => Some(Command::Accent),
            "staccato" => Some(Command::Staccato),
            // Slash commands apply to the PREVIOUS chord, so these are "after" variants
            "stop" => Some(Command::StopAfter),
            "stopgroove" | "stop_groove" => Some(Command::StopGrooveAfter),
            _ => None,
        }
    }

    /// Get the symbol representation of this command
    pub fn symbol(&self) -> &'static str {
        match self {
            Command::Fermata => "𝄐",                        // Unicode fermata symbol
            Command::Accent | Command::AccentOnPush => ">", // Accent symbol
            Command::Staccato => ".",                       // Staccato dot
            Command::Stop | Command::StopAfter => "🛑",     // Stop sign
            Command::StopGroove | Command::StopGrooveAfter => "⭕", // Stop groove (circle)
        }
    }

    /// Get the display name of this command
    pub fn name(&self) -> &'static str {
        match self {
            Command::Fermata => "fermata",
            Command::Accent => "accent",
            Command::AccentOnPush => "accent_on_push",
            Command::Staccato => "staccato",
            Command::Stop => "stop",
            Command::StopAfter => "stop_after",
            Command::StopGroove => "stop_groove",
            Command::StopGrooveAfter => "stop_groove_after",
        }
    }

    /// Check if this is any type of accent command
    pub fn is_accent(&self) -> bool {
        matches!(self, Command::Accent | Command::AccentOnPush)
    }

    /// Check if this is any type of stop command
    pub fn is_stop(&self) -> bool {
        matches!(
            self,
            Command::Stop | Command::StopAfter | Command::StopGroove | Command::StopGrooveAfter
        )
    }

    /// Check if this is a stop sign (octagon shape, not groove)
    pub fn is_stop_sign(&self) -> bool {
        matches!(self, Command::Stop | Command::StopAfter)
    }

    /// Check if this is a stop groove (circle shape)
    pub fn is_stop_groove(&self) -> bool {
        matches!(self, Command::StopGroove | Command::StopGrooveAfter)
    }

    /// Check if this stop command renders after the chord (vs before)
    pub fn is_stop_after(&self) -> bool {
        matches!(self, Command::StopAfter | Command::StopGrooveAfter)
    }

    /// Convert a "before" stop command to an "after" variant
    pub fn to_stop_after(self) -> Self {
        match self {
            Command::Stop => Command::StopAfter,
            Command::StopGroove => Command::StopGrooveAfter,
            other => other,
        }
    }

    /// Parse a stop command from `!`-prefixed syntax (e.g., "!STOP", "!STOPGROOVE")
    /// Returns the "before" variant; caller converts to "after" if needed.
    pub fn parse_stop_token(token: &str) -> Option<Self> {
        let upper = token.to_uppercase();
        match upper.as_str() {
            "!STOP" => Some(Command::Stop),
            "!STOPGROOVE" | "!STOP_GROOVE" => Some(Command::StopGroove),
            _ => None,
        }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fermata() {
        assert_eq!(Command::parse_slash("/fermata"), Some(Command::Fermata));
        assert_eq!(Command::parse_slash("/FERMATA"), Some(Command::Fermata));
        assert_eq!(Command::parse_slash("  /fermata  "), Some(Command::Fermata));
    }

    #[test]
    fn test_parse_accent() {
        assert_eq!(Command::parse_slash("/accent"), Some(Command::Accent));
        assert_eq!(Command::parse_slash("/ACCENT"), Some(Command::Accent));
    }

    #[test]
    fn test_parse_invalid() {
        assert_eq!(Command::parse_slash("/unknown"), None);
        assert_eq!(Command::parse_slash("not_a_command"), None);
    }

    #[test]
    fn test_symbols() {
        assert_eq!(Command::Fermata.symbol(), "𝄐");
        assert_eq!(Command::Accent.symbol(), ">");
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Command::Fermata), "𝄐");
        assert_eq!(format!("{}", Command::Accent), ">");
    }

    #[test]
    fn test_parse_stop_token() {
        assert_eq!(Command::parse_stop_token("!STOP"), Some(Command::Stop));
        assert_eq!(Command::parse_stop_token("!stop"), Some(Command::Stop));
        assert_eq!(Command::parse_stop_token("!Stop"), Some(Command::Stop));
        assert_eq!(
            Command::parse_stop_token("!STOPGROOVE"),
            Some(Command::StopGroove)
        );
        assert_eq!(
            Command::parse_stop_token("!stopgroove"),
            Some(Command::StopGroove)
        );
        assert_eq!(
            Command::parse_stop_token("!STOP_GROOVE"),
            Some(Command::StopGroove)
        );
        assert_eq!(Command::parse_stop_token("!OTHER"), None);
        assert_eq!(Command::parse_stop_token("STOP"), None);
    }

    #[test]
    fn test_parse_slash_stop() {
        // Slash commands apply to previous chord, so they produce "after" variants
        assert_eq!(Command::parse_slash("/stop"), Some(Command::StopAfter));
        assert_eq!(
            Command::parse_slash("/stopgroove"),
            Some(Command::StopGrooveAfter)
        );
    }

    #[test]
    fn test_is_stop() {
        assert!(Command::Stop.is_stop());
        assert!(Command::StopAfter.is_stop());
        assert!(Command::StopGroove.is_stop());
        assert!(Command::StopGrooveAfter.is_stop());
        assert!(!Command::Accent.is_stop());
        assert!(!Command::Fermata.is_stop());
    }

    #[test]
    fn test_stop_before_after() {
        assert!(!Command::Stop.is_stop_after());
        assert!(Command::StopAfter.is_stop_after());
        assert!(!Command::StopGroove.is_stop_after());
        assert!(Command::StopGrooveAfter.is_stop_after());

        assert!(Command::Stop.is_stop_sign());
        assert!(Command::StopAfter.is_stop_sign());
        assert!(!Command::StopGroove.is_stop_sign());

        assert!(Command::StopGroove.is_stop_groove());
        assert!(Command::StopGrooveAfter.is_stop_groove());
        assert!(!Command::Stop.is_stop_groove());
    }

    #[test]
    fn test_to_stop_after() {
        assert_eq!(Command::Stop.to_stop_after(), Command::StopAfter);
        assert_eq!(
            Command::StopGroove.to_stop_after(),
            Command::StopGrooveAfter
        );
        // Non-stop commands pass through unchanged
        assert_eq!(Command::Accent.to_stop_after(), Command::Accent);
    }
}
