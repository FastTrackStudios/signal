//! Error types for Logic Pro session parsing.

use std::path::PathBuf;

/// Errors that can occur when parsing a `.logicx` bundle.
#[derive(Debug, thiserror::Error)]
pub enum LogicError {
    /// I/O error reading the bundle or one of its files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The path does not look like a `.logicx` bundle directory.
    #[error("not a valid .logicx bundle: {0}")]
    NotABundle(PathBuf),

    /// A plist file has an unexpected structure (missing or wrong-typed key).
    #[error("unexpected plist structure in {path}: {reason}")]
    PlistStructure { path: PathBuf, reason: String },

    /// A required file is missing from inside the bundle.
    #[error("missing required file in bundle: {0}")]
    MissingFile(PathBuf),

    /// A plist file could not be parsed.
    #[error("plist error in {path}: {source}")]
    Plist {
        path: PathBuf,
        #[source]
        source: plist::Error,
    },

    /// The ProjectData binary is too short to contain a valid file header.
    #[error("ProjectData too short: {len} bytes (need at least 24)")]
    TooShort { len: usize },

    /// The ProjectData magic bytes are wrong.
    #[error("unexpected ProjectData magic: {actual:#06x} (expected 0x2347)")]
    BadMagic { actual: u16 },

    /// A chunk header extends past the end of the file.
    #[error("truncated chunk header at offset {offset:#x}")]
    TruncatedChunkHeader { offset: usize },

    /// A chunk data section extends past the end of the file.
    #[error("chunk '{chunk_type}' at {offset:#x}: data extends {size} bytes past EOF")]
    TruncatedChunkData {
        chunk_type: String,
        offset: usize,
        size: u64,
    },
}

/// Result alias for Logic parsing operations.
pub type LogicResult<T> = Result<T, LogicError>;
