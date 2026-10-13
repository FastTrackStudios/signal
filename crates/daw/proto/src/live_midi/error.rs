//! Live MIDI service errors

use facet::Facet;

/// Errors that can occur during live MIDI operations
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum LiveMidiError {
    /// Device not found
    DeviceNotFound(u32),
    /// Device is not available
    DeviceUnavailable(u32),
    /// Device is already open
    DeviceAlreadyOpen(u32),
    /// Device is not open
    DeviceNotOpen(u32),
    /// Failed to open device
    OpenFailed(String),
    /// Failed to send MIDI
    SendFailed(String),
    /// Invalid MIDI message
    InvalidMessage(String),
    /// Operation not supported
    NotSupported(String),
    /// Internal error
    Internal(String),
}

impl std::fmt::Display for LiveMidiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceNotFound(id) => write!(f, "MIDI device {} not found", id),
            Self::DeviceUnavailable(id) => write!(f, "MIDI device {} is not available", id),
            Self::DeviceAlreadyOpen(id) => write!(f, "MIDI device {} is already open", id),
            Self::DeviceNotOpen(id) => write!(f, "MIDI device {} is not open", id),
            Self::OpenFailed(msg) => write!(f, "Failed to open MIDI device: {}", msg),
            Self::SendFailed(msg) => write!(f, "Failed to send MIDI: {}", msg),
            Self::InvalidMessage(msg) => write!(f, "Invalid MIDI message: {}", msg),
            Self::NotSupported(msg) => write!(f, "Operation not supported: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}
