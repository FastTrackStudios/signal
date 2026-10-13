//! Error types for DawProject file parsing.

/// Errors that can occur when parsing a DawProject file.
#[derive(Debug, thiserror::Error)]
pub enum DawProjectError {
    /// File is not a valid ZIP archive.
    #[error("not a valid ZIP archive")]
    NotZip,

    /// Required `project.xml` not found inside the archive.
    #[error("missing project.xml in archive")]
    MissingProjectXml,

    /// XML parse error.
    #[error("XML parse error: {0}")]
    Xml(String),

    /// Root element is not `<Project>`.
    #[error("missing <Project> root element")]
    MissingRoot,

    /// Format version is not supported.
    #[error("unsupported DawProject version: {0:?}")]
    UnsupportedVersion(String),

    /// A required element or attribute was missing.
    #[error("missing {0}")]
    Missing(&'static str),

    /// I/O error reading the file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// ZIP extraction error.
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
}

/// Result type alias for DawProject parsing operations.
pub type DawProjectResult<T> = Result<T, DawProjectError>;
