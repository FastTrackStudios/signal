//! Error types for Ableton Live set parsing.

/// Errors that can occur when parsing an Ableton Live set file.
#[derive(Debug, thiserror::Error)]
pub enum AbletonError {
    /// Pre-8.2 Ableton format detected (unsupported legacy binary format).
    #[error("pre-8.2 Ableton format (magic bytes 0xAB 0x1E) — not supported")]
    PreVersion8,

    /// File does not have a valid gzip header.
    #[error("not a gzip file (expected magic bytes 1f 8b)")]
    NotGzip,

    /// Decompressed data is not valid XML.
    #[error("XML parse error: {0}")]
    Xml(String),

    /// Root element is not `<Ableton>`.
    #[error("missing <Ableton> root element")]
    MissingRoot,

    /// Missing `<LiveSet>` element.
    #[error("missing <LiveSet> element")]
    MissingLiveSet,

    /// Version string could not be parsed.
    #[error("invalid version string: {0:?}")]
    InvalidVersion(String),

    /// Ableton version is too old to parse.
    #[error("unsupported Ableton version: {major}.{minor} (minimum supported: 8.0)")]
    UnsupportedVersion { major: u32, minor: u32 },

    /// A required element was missing from the XML.
    #[error("missing element: {0}")]
    MissingElement(&'static str),

    /// I/O error reading the file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for Ableton parsing operations.
pub type AbletonResult<T> = Result<T, AbletonError>;
