//! Sampler error type — shared by the pack/cache/stream engine and its
//! consumers (signal-sampler re-exports this as `signal_sampler::SamplerError`).

/// Errors from spec loading, pack decoding, and sample caching.
#[derive(Debug, thiserror::Error)]
pub enum SamplerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("audio probe: {0}")]
    Probe(String),

    #[error("audio decode: {0}")]
    Decode(String),

    #[error("spec parse error: {0}")]
    SpecParse(String),

    #[error("invalid MIDI note name: {0:?}")]
    BadNoteName(String),

    #[error("spec missing section {0:?}")]
    MissingSection(String),

    #[error("spec missing articulation {0:?}")]
    MissingArticulation(String),
}

impl SamplerError {
    /// The bytes a stream needs have not arrived yet (a streamed proxy
    /// read past what was fetched) — not a failure: try again later.
    #[must_use]
    pub fn is_not_yet(&self) -> bool {
        matches!(self, Self::Io(e) if e.kind() == std::io::ErrorKind::WouldBlock)
    }
}
