//! Conversion from Logic Pro types to `daw_proto` types.

use crate::types::LogicSession;
use daw_proto::capability::{Capability, FeatureSupport};

/// The capability declaration for the Logic Pro file format.
pub fn feature_support() -> FeatureSupport {
    FeatureSupport::new().read_only(&[
        Capability::Project,
        Capability::Tracks,
        Capability::Markers,
        Capability::TempoMap,
    ])
}

/// Human-readable summary of a parsed Logic Pro session.
pub fn session_summary(session: &LogicSession) -> String {
    let audio = session
        .tracks
        .iter()
        .filter(|t| t.kind == crate::types::TrackKind::Audio)
        .count();
    let midi = session
        .tracks
        .iter()
        .filter(|t| {
            matches!(
                t.kind,
                crate::types::TrackKind::Midi | crate::types::TrackKind::SoftwareInstrument
            )
        })
        .count();

    format!(
        "Logic Pro session '{}' @ {:.1} BPM, {}Hz, {}/{}, key {} {}: \
         {} track(s) ({} audio, {} MIDI/inst), {} marker(s), \
         {} tempo event(s), {} summing group(s), {} raw chunk(s)",
        session.variant_name,
        session.bpm,
        session.sample_rate,
        session.time_sig_numerator,
        session.time_sig_denominator,
        session.key,
        session.key_gender,
        session.tracks.len(),
        audio,
        midi,
        session.markers.len(),
        session.tempo_events.len(),
        session.summing_groups.len(),
        session.chunks.len(),
    )
}
