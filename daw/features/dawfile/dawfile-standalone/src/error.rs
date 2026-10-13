//! Errors for the `.session` format.

use thiserror::Error;

/// Anything that can go wrong reading, writing or converting a `.session`.
#[derive(Debug, Error)]
pub enum DawError {
    /// Filesystem failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// The `.session` text did not parse as styx, or did not match the schema.
    #[error("parsing {path}: {message}")]
    Parse {
        /// What was being parsed (a file path, or `<memory>`).
        path: String,
        /// The underlying styx / facet diagnostic.
        message: String,
    },

    /// The persisted CRDT history could not be exported, imported or
    /// merged. Never fatal to opening a project: a bad oplog costs
    /// merge history, and the text is the source of truth.
    #[error("oplog: {0}")]
    Oplog(String),

    /// Serializing the document to styx failed.
    #[error("serializing: {0}")]
    Serialize(String),

    /// A project directory did not have the shape the format requires:
    /// exactly `<name>.session` and `objects/`.
    #[error("{path} is not a .session project: {reason}")]
    NotAProject {
        /// The directory inspected.
        path: String,
        /// Why it was rejected.
        reason: String,
    },

    /// The manifest references an object that is not in `objects/`.
    ///
    /// This is the loud failure #155 decision 7 requires: a project whose
    /// large data has not finished syncing must not open half-broken.
    #[error("missing object {id} (referenced as {referenced_as})")]
    MissingObject {
        /// The content hash that could not be resolved.
        id: String,
        /// What referred to it, for a message a human can act on.
        referenced_as: String,
    },

    /// The referenced entity id is not in the document.
    #[error("no such {kind}: {id}")]
    NoSuchEntity {
        /// `track` / `item` / `take` / `envelope`.
        kind: &'static str,
        /// The id that did not resolve.
        id: String,
    },

    /// Reading or writing REAPER `.rpp`.
    #[error("rpp: {0}")]
    Rpp(String),
}

/// Result alias for this crate.
pub type DawResult<T> = Result<T, DawError>;
