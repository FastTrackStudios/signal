//! What the core asks of whoever runs it.
//!
//! Studio's engine implements [`SignalHost`] directly when it links the
//! core; the CLAP implements it over the `com.fasttrackstudio.host-actions/1`
//! host extension where the host has it, and over the local disk where it
//! doesn't (REAPER, Bitwig: no host actions, files by path). Every call is
//! made off the audio thread, while a graph is built or by an action; the
//! processor itself never calls the host.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

/// A file a block needs (a NAM capture, a cab IR, a sample): what a preset
/// stores, and what the host resolves. Captures are private (AGENTS.md), so
/// a preset only ever names them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetRef {
    /// A path, or a host-defined reference (`rig-library:guitar/models/…`).
    pub path: String,
    /// The content hash, when the preset knows it: lets a host find the same
    /// file under another path, and refuse a different one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blake3: Option<String>,
}

impl AssetRef {
    #[must_use]
    pub fn path(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            blake3: None,
        }
    }
}

/// Why a host call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    /// This host doesn't do that (a DAW without the host-actions extension).
    Unsupported,
    NotFound(String),
    Failed(String),
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("not supported by this host"),
            Self::NotFound(what) => write!(f, "not found: {what}"),
            Self::Failed(why) => write!(f, "{why}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for HostError {}

/// One of the host's actions, as its action list describes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionInfo {
    /// `HITS_MIDI`, `QUANTIZE`, `TRACK_SET`, …
    pub id: String,
    pub description: String,
    /// JSON schema of the arguments, when the host has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_schema: Option<String>,
}

/// A drum hit: when, and how hard.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    /// Seconds from the song's start.
    pub time: f64,
    /// 0–1.
    pub velocity: f32,
}

/// A tempo change.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TempoPoint {
    /// Seconds from the song's start.
    pub time: f64,
    pub bpm: f64,
    pub numerator: u8,
    pub denominator: u8,
}

/// The host's side of Signal.
pub trait SignalHost: Send + Sync {
    /// The bytes of a file a block needs.
    ///
    /// # Errors
    ///
    /// [`HostError::NotFound`] when the host has no such file.
    fn load_file(&self, asset: &AssetRef) -> Result<Vec<u8>, HostError>;

    /// The host's actions (Studio's Action List).
    ///
    /// # Errors
    ///
    /// [`HostError::Unsupported`] in a host without actions.
    fn list_actions(&self) -> Result<Vec<ActionInfo>, HostError> {
        Err(HostError::Unsupported)
    }

    /// Run one of the host's actions: JSON arguments in, JSON result out.
    ///
    /// # Errors
    ///
    /// [`HostError::Unsupported`] in a host without actions; the action's
    /// own failure otherwise.
    fn run_action(&self, id: &str, args_json: &str) -> Result<String, HostError> {
        let _ = (id, args_json);
        Err(HostError::Unsupported)
    }

    /// The song's hits for a piece (`Drums/Kick`), from the host's hit
    /// markers.
    ///
    /// # Errors
    ///
    /// [`HostError::Unsupported`] in a host that doesn't know hits.
    fn hits(&self, piece: &str) -> Result<Vec<Hit>, HostError> {
        let _ = piece;
        Err(HostError::Unsupported)
    }

    /// The song's tempo map.
    ///
    /// # Errors
    ///
    /// [`HostError::Unsupported`] in a host that doesn't share it (a plugin
    /// sees the tempo per block through its transport instead).
    fn tempo_map(&self) -> Result<Vec<TempoPoint>, HostError> {
        Err(HostError::Unsupported)
    }
}

/// A host with nothing: no files, no actions. A chain of Signal's own
/// blocks that need no files still builds.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHost;

impl SignalHost for NoHost {
    fn load_file(&self, asset: &AssetRef) -> Result<Vec<u8>, HostError> {
        Err(HostError::NotFound(asset.path.clone()))
    }
}

/// Files from a map of path → bytes: the browser (models arrive over
/// HTTP before the graph is built) and tests.
#[derive(Clone, Debug, Default)]
pub struct MemoryHost {
    files: alloc::collections::BTreeMap<String, Vec<u8>>,
}

impl MemoryHost {
    #[must_use]
    pub fn with(mut self, path: impl Into<String>, bytes: Vec<u8>) -> Self {
        self.files.insert(path.into(), bytes);
        self
    }
}

impl SignalHost for MemoryHost {
    fn load_file(&self, asset: &AssetRef) -> Result<Vec<u8>, HostError> {
        self.files
            .get(&asset.path)
            .cloned()
            .ok_or_else(|| HostError::NotFound(asset.path.clone()))
    }
}
