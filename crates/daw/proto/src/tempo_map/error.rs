//! Tempo map error types

use facet::Facet;
use std::fmt;

/// Errors that can occur during tempo map operations
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Facet)]
pub enum TempoMapError {
    /// Invalid tempo value (must be > 0)
    InvalidTempo(f64),
    /// Invalid time signature
    InvalidTimeSignature { numerator: i32, denominator: i32 },
    /// Invalid position (e.g., negative time)
    InvalidPosition(f64),
    /// Tempo point not found at the given index
    PointNotFound(u32),
    /// Operation failed with a message
    OperationFailed(String),
}

impl fmt::Display for TempoMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTempo(bpm) => write!(f, "Invalid tempo: {} BPM", bpm),
            Self::InvalidTimeSignature {
                numerator,
                denominator,
            } => {
                write!(f, "Invalid time signature: {}/{}", numerator, denominator)
            }
            Self::InvalidPosition(pos) => write!(f, "Invalid position: {}", pos),
            Self::PointNotFound(idx) => write!(f, "Tempo point not found at index: {}", idx),
            Self::OperationFailed(msg) => write!(f, "Operation failed: {}", msg),
        }
    }
}

impl std::error::Error for TempoMapError {}
