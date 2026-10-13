//! Shared expression-editor application services, independent of a window or renderer.
//! Backends stay on their owner thread; only captured audio enters workers.
mod analysis;
/// Detected hits, kept on disk so a reopen is not a re-analysis.
mod analysis_cache;
mod audio_read;
mod controller;
pub mod drum_host;
mod timeline;
mod workspace;

pub(crate) use analysis::{blend, percussion_doc};
pub use audio_read::{read_take_mono, track_timeline};
pub use drum_host::{DrumHost, HostLane, SharedDrumHost};
pub use timeline::{attach_timeline, bar_grid};
pub use workspace::{DrumWorkspace, drum_workspace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    NoKitFolder {
        project: String,
        wanted: Option<String>,
    },
    NoEditableItem {
        project: String,
        items: usize,
    },
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoKitFolder {
                project,
                wanted: Some(wanted),
            } => write!(f, "{project} has no kit folder named {wanted:?}"),
            Self::NoKitFolder {
                project,
                wanted: None,
            } => write!(f, "{project} has no drum kit folder"),
            Self::NoEditableItem { project, items } => {
                write!(f, "{project} has {items} items but no editable drum audio")
            }
        }
    }
}
impl std::error::Error for WorkspaceError {}

pub use expression_editor_audio::daw_bound::DrumDaw;
