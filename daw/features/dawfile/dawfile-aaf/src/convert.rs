//! Conversion from AAF types to `daw_proto` types.
//!
//! Maps [`AafSession`] data to the format-agnostic protocol types used by
//! the rest of the DAW system.

use crate::types::AafSession;
use daw_proto::capability::{Capability, FeatureSupport};

/// The capability declaration for the AAF file format.
pub fn feature_support() -> FeatureSupport {
    FeatureSupport::new()
        .read_write(&[
            Capability::Project,
            Capability::Tracks,
            Capability::Items,
            Capability::Markers,
        ])
        .read_only(&[
            Capability::Takes,
            Capability::Regions,
            Capability::TempoMap, // TODO: not yet parsed, but AAF carries tempo markers
            Capability::Automation, // TODO: ControlPoints on OperationGroups (gain ramps, etc.)
        ])
}

/// Human-readable summary of a parsed AAF session.
pub fn session_summary(session: &AafSession) -> String {
    let audio_tracks = session
        .tracks
        .iter()
        .filter(|t| t.kind == crate::types::TrackKind::Audio)
        .count();
    let video_tracks = session
        .tracks
        .iter()
        .filter(|t| t.kind == crate::types::TrackKind::Video)
        .count();
    let total_clips: usize = session.tracks.iter().map(|t| t.clips.len()).sum();

    format!(
        "AAF session @ {}Hz: {} audio track(s), {} video track(s), {} clip(s), \
         {} marker(s), {} composition(s)",
        session.session_sample_rate,
        audio_tracks,
        video_tracks,
        total_clips,
        session.markers.len(),
        session.compositions.len(),
    )
}
