//! MIDI editing service errors

use facet::Facet;

/// Errors that can occur during MIDI editing operations
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum MidiError {
    /// Take not found
    TakeNotFound(String),
    /// Item not found
    ItemNotFound(String),
    /// Take is not a MIDI take
    NotMidiTake(String),
    /// Note not found
    NoteNotFound(u32),
    /// CC event not found
    CcNotFound(u32),
    /// Event not found
    EventNotFound(u32),
    /// Invalid pitch (must be 0-127)
    InvalidPitch(u8),
    /// Invalid velocity (must be 1-127)
    InvalidVelocity(u8),
    /// Invalid channel (must be 0-15)
    InvalidChannel(u8),
    /// Invalid controller (must be 0-127)
    InvalidController(u8),
    /// Invalid position
    InvalidPosition(String),
    /// Operation not supported
    NotSupported(String),
    /// Internal error
    Internal(String),
}

impl std::fmt::Display for MidiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TakeNotFound(msg) => write!(f, "Take not found: {}", msg),
            Self::ItemNotFound(msg) => write!(f, "Item not found: {}", msg),
            Self::NotMidiTake(msg) => write!(f, "Not a MIDI take: {}", msg),
            Self::NoteNotFound(idx) => write!(f, "Note not found at index {}", idx),
            Self::CcNotFound(idx) => write!(f, "CC event not found at index {}", idx),
            Self::EventNotFound(idx) => write!(f, "Event not found at index {}", idx),
            Self::InvalidPitch(p) => write!(f, "Invalid pitch {}: must be 0-127", p),
            Self::InvalidVelocity(v) => write!(f, "Invalid velocity {}: must be 1-127", v),
            Self::InvalidChannel(c) => write!(f, "Invalid channel {}: must be 0-15", c),
            Self::InvalidController(c) => write!(f, "Invalid controller {}: must be 0-127", c),
            Self::InvalidPosition(msg) => write!(f, "Invalid position: {}", msg),
            Self::NotSupported(msg) => write!(f, "Operation not supported: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}
