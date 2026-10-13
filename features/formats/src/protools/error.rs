//! Error types for Pro Tools session parsing.

/// Errors that can occur when parsing a Pro Tools session file.
#[derive(Debug, thiserror::Error)]
pub enum PtError {
    /// File is too short to contain a valid header.
    #[error("file too short ({0} bytes, minimum is 20)")]
    FileTooShort(usize),

    /// Unrecognized encryption type byte.
    #[error("unsupported encryption type: 0x{0:02x}")]
    UnsupportedEncryption(u8),

    /// File does not have a valid Pro Tools signature.
    #[error("invalid file signature")]
    InvalidSignature,

    /// Pro Tools version is not in the supported range (5-12).
    #[error("unsupported Pro Tools version: {0}")]
    UnsupportedVersion(u16),

    /// Failed to parse a required structure from the binary data.
    #[error("parse error at offset 0x{offset:x}: {message}")]
    ParseError { offset: usize, message: String },

    /// I/O error reading the session file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for Pro Tools parsing operations.
pub type PtResult<T> = Result<T, PtError>;
