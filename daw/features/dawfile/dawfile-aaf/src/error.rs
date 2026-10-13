//! Error types for AAF session parsing.

use std::path::PathBuf;

/// Errors that can occur when parsing an AAF file.
#[derive(Debug, thiserror::Error)]
pub enum AafError {
    /// I/O error reading the file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The file is not a valid AAF compound document.
    #[error("not a valid AAF compound document: {reason}")]
    InvalidFile { reason: String },

    /// A required AAF object was not found in the CFB.
    #[error("AAF object not found at CFB path: {0}")]
    ObjectNotFound(PathBuf),

    /// A required property is missing from an object.
    #[error("missing required property 0x{pid:04X} on object at {path:?}")]
    MissingProperty { pid: u16, path: PathBuf },

    /// The property stream has an unrecognised byte order mark.
    #[error("unsupported byte order mark in properties stream at {path:?}: {bom:#06x}")]
    UnsupportedByteOrder { bom: u16, path: PathBuf },

    /// A property value has an unexpected size.
    #[error("property 0x{pid:04X} at {path:?}: expected {expected} bytes, got {actual}")]
    PropertySizeMismatch {
        pid: u16,
        path: PathBuf,
        expected: usize,
        actual: usize,
    },

    /// A string property is not valid UTF-16LE.
    #[error("invalid UTF-16 string in property 0x{pid:04X} at {path:?}")]
    InvalidString { pid: u16, path: PathBuf },

    /// The properties stream is truncated mid-entry.
    #[error("truncated property stream at offset {offset} in {path:?}")]
    TruncatedStream { offset: usize, path: PathBuf },
}

/// Result type alias for AAF parsing operations.
pub type AafResult<T> = Result<T, AafError>;
