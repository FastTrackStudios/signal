//! Conversion from DawProject types to `daw_proto` types.
//!
//! Declares format capability support and provides summary utilities.

use crate::types::DawProject;
use daw_proto::capability::{Capability, FeatureSupport};

/// The capability declaration for the DawProject parser/writer.
///
/// DawProject is a rich open exchange format — we support both reading and
/// writing the full set of timeline and mixer data it defines.
pub fn feature_support() -> FeatureSupport {
    FeatureSupport::new().read_write(&[
        Capability::Project,
        Capability::Tracks,
        Capability::TrackRouting,
        Capability::Items,
        Capability::Midi,
        Capability::FxChain,
        Capability::FxState,
        Capability::Automation,
        Capability::Markers,
        Capability::TempoMap,
    ])
}

/// Summary of a parsed DawProject, suitable for display.
pub fn project_summary(project: &DawProject) -> String {
    let app = project
        .application
        .as_ref()
        .map(|a| format!("{} {}", a.name, a.version))
        .unwrap_or_else(|| "unknown application".to_string());

    let title = project
        .metadata
        .as_ref()
        .and_then(|m| m.title.as_deref())
        .unwrap_or("untitled");

    let track_count = count_tracks(&project.tracks);

    format!(
        "DawProject v{} from {}: \"{title}\" @ {:.1} BPM ({}/{}), {} tracks",
        project.version,
        app,
        project.transport.tempo,
        project.transport.numerator,
        project.transport.denominator,
        track_count,
    )
}

fn count_tracks(tracks: &[crate::types::Track]) -> usize {
    tracks
        .iter()
        .fold(0, |acc, t| acc + 1 + count_tracks(&t.children))
}
