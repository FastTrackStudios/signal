use facet::Facet;

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Facet)]
pub enum TransportError {
    InvalidTempo(String),

    InvalidTimeSignature(String),

    NotReady(String),

    RecordingError(String),

    InvalidPosition(String),

    InvalidPlayrate(String),

    LockError(String),

    InvalidState(String),

    EventBroadcast(String),

    Internal(String),
}
impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::InvalidTempo(msg) => write!(f, "Invalid tempo: {}", msg),
            TransportError::InvalidTimeSignature(msg) => {
                write!(f, "Invalid time signature: {}", msg)
            }
            TransportError::NotReady(msg) => write!(f, "Not ready: {}", msg),
            TransportError::RecordingError(msg) => write!(f, "Recording error: {}", msg),
            TransportError::InvalidPosition(msg) => write!(f, "Invalid position: {}", msg),
            TransportError::InvalidPlayrate(msg) => write!(f, "Invalid playrate: {}", msg),
            TransportError::LockError(msg) => write!(f, "Lock error: {}", msg),
            TransportError::InvalidState(msg) => write!(f, "Invalid state: {}", msg),
            TransportError::EventBroadcast(msg) => write!(f, "Event broadcast error: {}", msg),
            TransportError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<std::io::Error> for TransportError {
    fn from(err: std::io::Error) -> Self {
        TransportError::Internal(err.to_string())
    }
}
